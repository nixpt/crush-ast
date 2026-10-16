//! Definite assignment of CASM locals (CRUSH-zorro-54, CRUSH-zorro-55).
//!
//! The VMs fail a `load` of a local that has no value yet ("load from
//! uninitialised slot"). The AOT backends, FastVM and the JIT used to read such
//! a local as `null` and carry on, printing a wrong answer. They now fail too,
//! and this pass keeps that cheap: a `load` that every path from the
//! function's entry reaches only after a `store` to the same name can't see an
//! unassigned local, so it needs no check. Only the remaining loads are
//! checked. AOT Rust, AOT C, crush-aotc and FastVM's lowering (which the JIT
//! compiles) all share this one implementation.
//!
//! It is a forward "must" data-flow analysis over the control flow the
//! engines run: a jump (`jmp`/`jump`, `break`, `continue`) goes to its
//! `target`; a conditional jump (`jmp_if`/`jmp_if_not` and their aliases) and
//! `enter_try` (whose `target` is the handler) go to the target or fall
//! through; `ret`/`return`/`halt` end the function; every other op falls
//! through. A `target` is an index into the body or the name of a `label`; if
//! one can't be resolved the analysis gives up and every load is checked.
//! Parameters need no special case: they arrive on the operand stack and the
//! body's leading `store`s assign them.
//!
//! The result is conservative. A load in code the analysis finds unreachable
//! is checked; so is one reached through an edge it over-approximates — a
//! handler is entered with what was assigned at its `enter_try` (a `throw`
//! later in the `try` can only have assigned more), and a `throw` is also
//! treated as falling through, which only removes assignments at the merge.

use crate::{Function, Instruction};
use std::collections::HashMap;

fn is_load(op: &str) -> bool {
    matches!(op, "load" | "load_local" | "get_local")
}

fn is_store(op: &str) -> bool {
    matches!(op, "store" | "store_local" | "set_local")
}

/// Where control can go after `body[pc]`, or `Err(())` when a jump's target
/// can't be resolved.
fn successors(body: &[Instruction], pc: usize, labels: &HashMap<&str, usize>) -> Result<[Option<usize>; 2], ()> {
    let instr = &body[pc];
    let target = || match instr.args.get("target") {
        Some(t) if t.is_u64() => t.as_u64().map(|t| t as usize).ok_or(()),
        Some(t) => t.as_str().and_then(|l| labels.get(l).copied()).ok_or(()),
        None => Err(()),
    };
    Ok(match instr.op.as_str() {
        "ret" | "return" | "halt" => [None, None],
        "jmp" | "jump" | "break" | "continue" => [Some(target()?), None],
        "jmp_if" | "jump_if" | "branch_if" | "jmp_if_not" | "jump_if_not" | "branch_if_not" | "enter_try" => {
            [Some(target()?), Some(pc + 1)]
        }
        _ => [Some(pc + 1), None],
    })
}

/// For each instruction of `func`, whether it is a `load` that may run before
/// its local has been assigned on some path, and so must be checked.
pub fn loads_to_check(func: &Function) -> Vec<bool> {
    let body = &func.body;
    let n = body.len();
    let name_of = |pc: usize| body[pc].args.get("name").and_then(|v| v.as_str());
    let every_load = || body.iter().map(|i| is_load(&i.op)).collect();

    // Dense index for each local name a load or store mentions, and the
    // position of each label.
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut labels: HashMap<&str, usize> = HashMap::new();
    for (pc, instr) in body.iter().enumerate() {
        if (is_load(&instr.op) || is_store(&instr.op))
            && let Some(name) = name_of(pc)
        {
            let next = index.len();
            index.entry(name).or_insert(next);
        } else if instr.op == "label"
            && let Some(name) = name_of(pc)
        {
            labels.entry(name).or_insert(pc);
        }
    }
    let words = index.len().div_ceil(64);
    let local = |pc: usize| name_of(pc).and_then(|nm| index.get(nm).copied());

    // `assigned[pc]`: locals assigned on every path reaching `pc` (`None`:
    // no path found yet). Sets only shrink, so this terminates.
    let mut assigned: Vec<Option<Vec<u64>>> = vec![None; n];
    let mut work: Vec<usize> = Vec::new();
    if n > 0 {
        assigned[0] = Some(vec![0; words]);
        work.push(0);
    }
    while let Some(pc) = work.pop() {
        let mut out = assigned[pc].clone().expect("queued state");
        if is_store(&body[pc].op)
            && let Some(i) = local(pc)
        {
            out[i / 64] |= 1 << (i % 64);
        }
        let Ok(succs) = successors(body, pc, &labels) else {
            return every_load();
        };
        // A successor at or past the end leaves the function.
        for s in succs.into_iter().flatten().filter(|&s| s < n) {
            match &mut assigned[s] {
                Some(cur) => {
                    let mut changed = false;
                    for (c, o) in cur.iter_mut().zip(&out) {
                        let m = *c & o;
                        changed |= m != *c;
                        *c = m;
                    }
                    if changed {
                        work.push(s);
                    }
                }
                slot @ None => {
                    *slot = Some(out.clone());
                    work.push(s);
                }
            }
        }
    }

    (0..n)
        .map(|pc| {
            is_load(&body[pc].op)
                && match (local(pc), &assigned[pc]) {
                    (Some(i), Some(set)) => set[i / 64] & (1 << (i % 64)) == 0,
                    _ => true,
                }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn func(body: serde_json::Value) -> Function {
        serde_json::from_value(serde_json::json!({ "params": [], "body": body })).unwrap()
    }

    fn checked(body: serde_json::Value) -> Vec<usize> {
        loads_to_check(&func(body)).iter().enumerate().filter(|(_, c)| **c).map(|(pc, _)| pc).collect()
    }

    #[test]
    fn a_load_after_a_store_is_not_checked() {
        let c = checked(serde_json::json!([
            {"op": "push_int", "value": 1}, {"op": "store", "name": "x"},
            {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert!(c.is_empty(), "{c:?}");
    }

    #[test]
    fn a_load_with_no_store_is_checked() {
        let c = checked(serde_json::json!([{"op": "load", "name": "x"}, {"op": "ret"}]));
        assert_eq!(c, [0]);
    }

    #[test]
    fn a_store_on_one_branch_does_not_dominate_the_merge() {
        // 0 push_bool; 1 jmp_if_not 4; 2 push_int; 3 store x; 4 load x; 5 ret
        let c = checked(serde_json::json!([
            {"op": "push_bool", "value": true}, {"op": "jmp_if_not", "target": 4},
            {"op": "push_int", "value": 7}, {"op": "store", "name": "x"},
            {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert_eq!(c, [4]);
    }

    #[test]
    fn stores_on_both_branches_dominate_the_merge() {
        // 0 push_bool; 1 jmp_if_not 5; 2 push 1; 3 store x; 4 jmp 7; 5 push 2; 6 store x; 7 load x; 8 ret
        let c = checked(serde_json::json!([
            {"op": "push_bool", "value": true}, {"op": "jmp_if_not", "target": 5},
            {"op": "push_int", "value": 1}, {"op": "store", "name": "x"}, {"op": "jmp", "target": 7},
            {"op": "push_int", "value": 2}, {"op": "store", "name": "x"},
            {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert!(c.is_empty(), "{c:?}");
    }

    #[test]
    fn a_store_later_in_a_loop_does_not_dominate_an_earlier_load() {
        // 0 load y (reads last iteration's y: unassigned on the first) ; 1 pop;
        // 2 push; 3 store y; 4 push_bool; 5 jmp_if 0; 6 push_null; 7 ret
        let c = checked(serde_json::json!([
            {"op": "load", "name": "y"}, {"op": "pop"},
            {"op": "push_int", "value": 1}, {"op": "store", "name": "y"},
            {"op": "push_bool", "value": true}, {"op": "jmp_if", "target": 0},
            {"op": "push_null"}, {"op": "ret"}
        ]));
        assert_eq!(c, [0]);
    }

    #[test]
    fn unreachable_loads_are_checked() {
        let c = checked(serde_json::json!([
            {"op": "push_null"}, {"op": "ret"}, {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert_eq!(c, [2]);
    }

    #[test]
    fn string_label_targets_are_followed() {
        // 0 push_bool; 1 jmp_if_not "skip"; 2 push 1; 3 store x; 4 label skip; 5 load x; 6 ret
        let c = checked(serde_json::json!([
            {"op": "push_bool", "value": true}, {"op": "jmp_if_not", "target": "skip"},
            {"op": "push_int", "value": 1}, {"op": "store", "name": "x"},
            {"op": "label", "name": "skip"}, {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert_eq!(c, [5]);
    }

    #[test]
    fn op_aliases_count_as_loads_and_stores() {
        let c = checked(serde_json::json!([
            {"op": "push_int", "value": 1}, {"op": "set_local", "name": "x"},
            {"op": "get_local", "name": "x"}, {"op": "load_local", "name": "y"}, {"op": "return"}
        ]));
        assert_eq!(c, [3]);
    }

    #[test]
    fn a_handler_sees_only_what_was_assigned_at_enter_try() {
        // 0 enter_try 5; 1 push; 2 store x; 3 exit_try; 4 jmp 6; 5 pop (error);
        // 6 load x; 7 ret — the handler path reaches 6 without x.
        let c = checked(serde_json::json!([
            {"op": "enter_try", "target": 5}, {"op": "push_int", "value": 1},
            {"op": "store", "name": "x"}, {"op": "exit_try"}, {"op": "jmp", "target": 6},
            {"op": "pop"}, {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert_eq!(c, [6]);
    }

    #[test]
    fn break_jumps_past_the_store_it_skips() {
        // 0 push_bool; 1 jmp_if_not 3; 2 break 5; 3 push; 4 store x; 5 load x; 6 ret
        let c = checked(serde_json::json!([
            {"op": "push_bool", "value": true}, {"op": "jmp_if_not", "target": 3},
            {"op": "break", "target": 5}, {"op": "push_int", "value": 1},
            {"op": "store", "name": "x"}, {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert_eq!(c, [5]);
    }

    #[test]
    fn an_unresolvable_target_checks_every_load() {
        let c = checked(serde_json::json!([
            {"op": "push_int", "value": 1}, {"op": "store", "name": "x"},
            {"op": "load", "name": "x"}, {"op": "jmp", "target": "nowhere"},
            {"op": "load", "name": "x"}, {"op": "ret"}
        ]));
        assert_eq!(c, [2, 4]);
    }
}
