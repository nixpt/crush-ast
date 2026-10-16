//! Definite assignment of CASM locals (CRUSH-zorro-54).
//!
//! The VMs fail a `load` of a local that has no value yet ("load from
//! uninitialised slot"). The AOT backends used to read such a local as `null`
//! and carry on, printing a wrong answer. They now fail too, and this pass
//! keeps that cheap: a `load` that every path from the function's entry
//! reaches only after a `store` to the same name can't see an unassigned
//! local, so it needs no check. Only the remaining loads are checked.
//!
//! It is a forward "must" data-flow analysis over the same control flow the
//! backends emit: `jmp`/`jmp_if`/`jmp_if_not` with a `target` branch, `ret` /
//! `halt` end the function, and every other op falls through. (An op a
//! backend can't translate fails generation, so it never runs.) Parameters
//! need no special case: they arrive on the operand stack and the body's
//! leading `store`s assign them. The result is conservative — a load in code
//! the analysis finds unreachable is checked too.

use std::collections::HashMap;

/// For each instruction of `func`, whether it is a `load` that may run before
/// its local has been assigned on some path, and so must be checked.
pub(crate) fn loads_to_check(func: &casm::Function) -> Vec<bool> {
    let body = &func.body;
    let n = body.len();
    let name_of = |pc: usize| body[pc].args.get("name").and_then(|v| v.as_str());

    // Dense index for each local name a load or store mentions.
    let mut index: HashMap<&str, usize> = HashMap::new();
    for (pc, instr) in body.iter().enumerate() {
        if matches!(instr.op.as_str(), "load" | "store")
            && let Some(name) = name_of(pc)
        {
            let next = index.len();
            index.entry(name).or_insert(next);
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
        let instr = &body[pc];
        if instr.op == "store"
            && let Some(i) = local(pc)
        {
            out[i / 64] |= 1 << (i % 64);
        }
        let target = instr.args.get("target").and_then(|v| v.as_u64()).map(|t| t as usize);
        let succs: [Option<usize>; 2] = match (instr.op.as_str(), target) {
            ("ret" | "halt", _) => [None, None],
            ("jmp", Some(t)) => [Some(t), None],
            ("jmp_if" | "jmp_if_not", Some(t)) => [Some(t), Some(pc + 1)],
            _ => [Some(pc + 1), None],
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
            body[pc].op == "load"
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

    fn func(body: serde_json::Value) -> casm::Function {
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
}
