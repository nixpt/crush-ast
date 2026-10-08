//! Which host capabilities a compiled program can ask for (CRUSH-170).
//!
//! Read off the CVM1 bytecode, so it sees exactly the names the VM will
//! gate on at run time, after every lowering: `CAP_CALL` names (every
//! `fs.cat(…)`, `io.print(…)`, and every builtin the compiler turns into a
//! capability call), `EXEC_LANG` → `polyglot.<lang>` (same gate name the
//! VM checks), and the AI / DOM opcodes → `ai_native.<kind>` /
//! `dom_native.<kind>`.
//!
//! Only code reachable from the entry function counts, so a dependency's
//! functions the program never calls don't add their capabilities.
//! Reachability follows `CALL` and treats any function whose name is pushed
//! as a string constant as reachable too, because `SPAWN` takes its target
//! name from the stack. A spawn target computed at run time (built by
//! concatenation, say) is missed; that is the one way this can under-report.
//! Capability names themselves are always constants in CVM1 (`CAP_CALL`
//! takes a const-pool index), so there are no dynamic capability names.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::bytecode::{self, OperandKind, Program};

#[derive(Default)]
struct FnFacts {
    caps: BTreeSet<String>,
    calls: Vec<String>,
    strings: Vec<String>,
}

/// Every capability name `program` can request from the VM, from the code
/// reachable from its entry function (`manifest.entry`, else `main`; the
/// whole program if neither exists). Errors on bytecode it cannot decode.
pub fn capabilities_used(program: &Program) -> Result<BTreeSet<String>, String> {
    let code = &program.code;
    let mut starts: Vec<(usize, &str)> = program
        .manifest
        .functions
        .iter()
        .map(|(name, f)| (f.entry, name.as_str()))
        .collect();
    starts.sort();
    let owner = |offset: usize| -> Option<&str> {
        match starts.partition_point(|(entry, _)| *entry <= offset) {
            0 => None,
            i => Some(starts[i - 1].1),
        }
    };
    let constant = |at: usize| -> Result<&str, String> {
        let idx = u16::from_be_bytes([code[at], code[at + 1]]) as usize;
        program
            .consts
            .get(idx)
            .map(String::as_str)
            .ok_or_else(|| format!("constant {idx} out of range at byte {}", at - 1))
    };

    // `None` = code before the first function (a prologue), always run.
    let mut facts: BTreeMap<Option<&str>, FnFacts> = BTreeMap::new();
    let mut ip = 0;
    while ip < code.len() {
        let op = code[ip];
        let kind = bytecode::operand_kind(op)
            .ok_or_else(|| format!("unknown opcode 0x{op:02x} at byte {ip}"))?;
        let next = ip + 1 + kind.byte_width();
        if next > code.len() {
            return Err(format!("truncated instruction at byte {ip}"));
        }
        let f = facts.entry(owner(ip)).or_default();
        match kind {
            OperandKind::Cap => {
                f.caps.insert(constant(ip + 1)?.to_string());
            }
            OperandKind::Func => f.calls.push(constant(ip + 1)?.to_string()),
            OperandKind::Str => {
                let text = constant(ip + 1)?;
                if op == bytecode::PUSH_STR {
                    f.strings.push(text.to_string());
                } else if op == bytecode::EXEC_LANG {
                    let lang = serde_json::from_str::<serde_json::Value>(text)
                        .ok()
                        .and_then(|spec| spec.get("lang")?.as_str().map(str::to_string))
                        .unwrap_or_else(|| "?".to_string());
                    f.caps.insert(crate::scheduler::polyglot_gate_name(&lang));
                } else if let Some(kind) = bytecode::ai_native_kind_for_opcode(op) {
                    f.caps.insert(format!("ai_native.{kind}"));
                } else if let Some(kind) = bytecode::dom_native_kind_for_opcode(op) {
                    f.caps.insert(format!("dom_native.{kind}"));
                }
            }
            _ => {}
        }
        ip = next;
    }

    let functions: HashSet<&str> = starts.iter().map(|(_, name)| *name).collect();
    let entry = program.manifest.entry.as_deref().unwrap_or("main");
    let mut todo: Vec<Option<&str>> = vec![None];
    if functions.contains(entry) {
        todo.push(Some(entry));
    } else {
        todo.extend(functions.iter().map(|name| Some(*name)));
    }
    let mut seen: HashSet<Option<&str>> = HashSet::new();
    let mut used = BTreeSet::new();
    while let Some(func) = todo.pop() {
        if !seen.insert(func) {
            continue;
        }
        let Some(f) = facts.get(&func) else { continue };
        used.extend(f.caps.iter().cloned());
        for name in f.calls.iter().chain(&f.strings) {
            if let Some(name) = functions.get(name.as_str()) {
                todo.push(Some(*name));
            }
        }
    }
    Ok(used)
}

#[cfg(test)]
mod tests {
    use super::capabilities_used;
    use crate::assembler::assemble;

    fn used(asm: &str) -> Vec<String> {
        let program = assemble(asm, None, None).expect("assemble");
        capabilities_used(&program)
            .expect("decode")
            .into_iter()
            .collect()
    }

    #[test]
    fn collects_cap_calls_from_reachable_functions_only() {
        let asm = r#"
.func main
    PUSH_STR "x"
    CAP_CALL "io.print" 1
    CALL helper
    HALT
.func helper
    PUSH_STR "a.txt"
    CAP_CALL "fs.cat" 1
    RET
.func unused
    CAP_CALL "net.http_get" 0
    RET
"#;
        assert_eq!(used(asm), ["fs.cat", "io.print"]);
    }

    #[test]
    fn a_function_named_by_a_string_constant_counts_as_reachable() {
        // SPAWN pops its target's name, so a pushed name may be a spawn.
        let asm = r#"
.func main
    PUSH_STR "worker"
    SPAWN 0
    HALT
.func worker
    CAP_CALL "time.now" 0
    RET
"#;
        assert_eq!(used(asm), ["time.now"]);
    }

    #[test]
    fn exec_lang_maps_to_the_gate_the_vm_checks() {
        let asm = r#"
.func main
    EXEC_LANG "{\"lang\":\"py\",\"code\":\"print(1)\",\"var_count\":0}"
    EXEC_LANG "{\"lang\":\"cobol\",\"code\":\"\",\"var_count\":0}"
    HALT
"#;
        assert_eq!(used(asm), ["polyglot.cobol", "polyglot.python"]);
    }
}
