use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};

static LABEL_COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_label() -> String {
    let n = LABEL_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("L{}", n)
}

fn get_slot(name: &str, map: &mut HashMap<String, u16>, next: &mut u16) -> u16 {
    if let Some(&slot) = map.get(name) {
        slot
    } else {
        let slot = *next;
        map.insert(name.to_string(), slot);
        *next += 1;
        slot
    }
}

fn cap_returns_value(name: &str) -> bool {
    if let Some(spec) = crush_vm::capabilities().get(name) {
        return spec.returns;
    }
    true
}

pub fn compile_crush_source(source: &str) -> anyhow::Result<crush_vm::Program> {
    let casm_program = crush_frontend::compile_crush_source(source)?;
    casm_to_vm(&casm_program)
}

pub fn casm_to_vm(program: &casm::Program) -> anyhow::Result<crush_vm::Program> {
    let mut lines: Vec<String> = Vec::new();
    let mut perms: HashSet<String> = HashSet::new();

    for (_fname, _func) in &program.functions {
        perms.extend(program.manifest.permissions.clone());
    }

    for (fname, func) in &program.functions {
        let mut slot_map: HashMap<String, u16> = HashMap::new();
        let mut next_slot: u16 = 0;
        let local_funcs: HashSet<String> = program.functions.keys().cloned().collect();

        lines.push(format!(".func {fname}"));

        let mut target_labels: HashMap<usize, String> = HashMap::new();
        for (_i, instr) in func.body.iter().enumerate() {
            if instr.op == "jmp" || instr.op == "jmp_if_not" || instr.op == "enter_try" {
                if let Some(target) = instr.args.get("target").and_then(|v| v.as_u64()) {
                    target_labels.entry(target as usize).or_insert_with(unique_label);
                }
            }
        }

        for (i, instr) in func.body.iter().enumerate() {
            if let Some(label) = target_labels.get(&i) {
                lines.push(format!("{label}:"));
            }

            let op = match instr.op.as_str() {
                "push_int" => {
                    let v = instr.args["value"].as_i64()
                        .ok_or_else(|| anyhow::anyhow!("push_int missing value at {fname}:{i}"))?;
                    format!("PUSH {v}")
                }
                "push_float" => {
                    let v = instr.args["value"].as_f64()
                        .ok_or_else(|| anyhow::anyhow!("push_float missing value at {fname}:{i}"))?;
                    format!("PUSH_F64 {v}")
                }
                "push_str" => {
                    let v = instr.args["value"].as_str()
                        .ok_or_else(|| anyhow::anyhow!("push_str missing value at {fname}:{i}"))?;
                    format!("PUSH_STR {v:?}")
                }
                "push_bool" => {
                    let v = instr.args["value"].as_bool()
                        .ok_or_else(|| anyhow::anyhow!("push_bool missing value at {fname}:{i}"))?;
                    format!("PUSH_BOOL {}", if v { 1 } else { 0 })
                }
                "push_null" => "PUSH_NULL".to_string(),
                "pop" => "POP".to_string(),
                "dup" => "DUP".to_string(),
                "load" => {
                    let name = instr.args["name"].as_str()
                        .ok_or_else(|| anyhow::anyhow!("load missing name at {fname}:{i}"))?;
                    let slot = get_slot(name, &mut slot_map, &mut next_slot);
                    format!("LOAD {slot}")
                }
                "store" => {
                    let name = instr.args["name"].as_str()
                        .ok_or_else(|| anyhow::anyhow!("store missing name at {fname}:{i}"))?;
                    let slot = get_slot(name, &mut slot_map, &mut next_slot);
                    format!("STORE {slot}")
                }
                "add" => "ADD".to_string(),
                "sub" => "SUB".to_string(),
                "mul" => "MUL".to_string(),
                "div" => "DIV".to_string(),
                "mod" => "MOD".to_string(),
                "eq" => "EQ".to_string(),
                "ne" => "EQ\n    NOT".to_string(),
                "lt" => "LT".to_string(),
                "gt" => "GT".to_string(),
                "le" => "GT\n    NOT".to_string(),
                "ge" => "LT\n    NOT".to_string(),
                "not" => "NOT".to_string(),
                "and" => {
                    let lend = unique_label();
                    let lfalse = unique_label();
                    format!("SWAP\n    DUP\n    JZ {lfalse}\n    POP\n    JMP {lend}\n{lfalse}:\n    POP\n    PUSH 0\n{lend}:")
                }
                "or" => {
                    let ltrue = unique_label();
                    let lend = unique_label();
                    format!("SWAP\n    DUP\n    JNZ {ltrue}\n    POP\n    JMP {lend}\n{ltrue}:\n    POP\n    PUSH 1\n{lend}:")
                }
                "call" => {
                    let fn_name = instr.args["function"].as_str()
                        .ok_or_else(|| anyhow::anyhow!("call missing function at {fname}:{i}"))?;
                    let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0);
                    if local_funcs.contains(fn_name) {
                        format!("CALL {fn_name}")
                    } else {
                        perms.insert(fn_name.to_string());
                        format!("CAP_CALL {fn_name:?} {argc}")
                    }
                }
                "cap_call" => {
                    let name = instr.args["name"].as_str()
                        .ok_or_else(|| anyhow::anyhow!("cap_call missing name at {fname}:{i}"))?;
                    let argc = instr.args["argc"].as_u64().unwrap_or(0);
                    perms.insert(name.to_string());
                    format!("CAP_CALL {name:?} {argc}")
                }
                "ret" => "RET".to_string(),
                "jmp" => {
                    let target = instr.args["target"].as_u64()
                        .ok_or_else(|| anyhow::anyhow!("jmp missing target at {fname}:{i}"))? as usize;
                    let label = target_labels.get(&target)
                        .ok_or_else(|| anyhow::anyhow!("jmp to unknown target {target} at {fname}:{i}"))?;
                    format!("JMP {label}")
                }
                "jmp_if_not" => {
                    let target = instr.args["target"].as_u64()
                        .ok_or_else(|| anyhow::anyhow!("jmp_if_not missing target at {fname}:{i}"))? as usize;
                    let label = target_labels.get(&target)
                        .ok_or_else(|| anyhow::anyhow!("jmp_if_not to unknown target {target} at {fname}:{i}"))?;
                    format!("JZ {label}")
                }
                "new_array" => "NEW_ARRAY 0".to_string(),
                "array_push" => "ARR_PUSH".to_string(),
                "array_pop" => "ARR_POP".to_string(),
                "len" => "ARR_LEN".to_string(),
                "index" => "ARR_GET".to_string(),
                "make_range" => "CAP_CALL \"make_range\" 2".to_string(),
                "str_contains" => "CAP_CALL \"str.contains\" 2".to_string(),
                "str_split" => "CAP_CALL \"str.split\" 2".to_string(),
                "str_replace" => "CAP_CALL \"str.replace\" 3".to_string(),
                "str_join" => "CAP_CALL \"str.join\" 2".to_string(),
                "arr_set" => "ARR_SET".to_string(),
                "export_var" => "PRINT".to_string(),
                "exec_lang" => {
                    let args_json = serde_json::to_string(&instr.args)
                        .map_err(|e| anyhow::anyhow!("exec_lang: failed to serialize args at {fname}:{i}: {e}"))?;
                    // Escape for assembly: \ → \\, " → \"
                    let esc = args_json.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("EXEC_LANG \"{esc}\"")
                }
                "throw" => "THROW".to_string(),
                "enter_try" => {
                    let target = instr.args["target"].as_u64()
                        .ok_or_else(|| anyhow::anyhow!("enter_try missing target at {fname}:{i}"))? as usize;
                    let label = target_labels.get(&target)
                        .ok_or_else(|| anyhow::anyhow!("enter_try to unknown target {target} at {fname}:{i}"))?;
                    format!("ENTER_TRY {label}")
                }
                "exit_try" => "EXIT_TRY".to_string(),
                "new_obj" => "NEW_OBJ".to_string(),
                "get_field" => {
                    let field = instr.args.get("field").or_else(|| instr.args.get("name")).and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("get_field missing field arg at {fname}:{i}"))?;
                    format!("GET_FIELD {field:?}")
                }
                "set_field" => {
                    let field = instr.args.get("field").or_else(|| instr.args.get("name")).and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("set_field missing field arg at {fname}:{i}"))?;
                    format!("SET_FIELD {field:?}")
                }
                other => anyhow::bail!("Unsupported CVM1 opcode: {other} at {fname}:{i}"),
            };
            lines.push(format!("    {op}"));
        }
    }

    // Post-process: suppress POP after non-returning CAP_CALL
    let mut cleaned: Vec<String> = Vec::new();
    let mut suppress_next_pop: Option<bool> = None;
    for line in &lines {
        let trimmed = line.trim();
        if let Some(cap_name) = trimmed.strip_prefix("CAP_CALL ") {
            let name = cap_name.split('"').nth(1).unwrap_or("");
            suppress_next_pop = Some(!cap_returns_value(name));
            cleaned.push(line.clone());
        } else if trimmed == "POP" {
            if let Some(suppress) = suppress_next_pop.take() {
                if suppress {
                    continue;
                }
            }
            suppress_next_pop = None;
            cleaned.push(line.clone());
        } else {
            suppress_next_pop = None;
            cleaned.push(line.clone());
        }
    }

    let assembly = cleaned.join("\n");
    let perms_slice: Vec<&str> = perms.iter().map(|s| s.as_str()).collect();
    let vm_program = crush_vm::assemble(
        &assembly,
        if perms_slice.is_empty() { None } else { Some(&perms_slice) },
        None,
    )?;
    Ok(vm_program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_simple_program() {
        let source = "fn main() {\n    io.print(\"hello from crush\")\n}\n";
        let program = compile_crush_source(source).expect("compilation failed");
        assert!(!program.code.is_empty(), "should produce bytecode");
        assert!(!program.consts.is_empty(), "should have constants");
    }

    #[test]
    fn test_compile_with_expression() {
        let source = "fn main() {\n    let x = 42\n    io.print(x)\n}\n";
        let program = compile_crush_source(source).expect("compilation failed");
        assert!(!program.code.is_empty());
    }

    #[test]
    fn test_compile_and_run() {
        let source = "fn main() {\n    io.print(\"hello from crush\")\n}\n";
        let prog = compile_crush_source(source).expect("compile");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run");
        assert_eq!(result.output, "hello from crush");
        assert!(result.halted);
    }

    #[test]
    fn test_compile_with_bool() {
        let source = "fn main() {\n    let a = true\n    let b = false\n    io.print(a)\n}\n";
        let prog = compile_crush_source(source).expect("compile bool");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run bool");
        assert_eq!(result.output, "true");
    }

    #[test]
    fn test_compile_with_if_bool_condition() {
        let source = "fn main() {\n    if true {\n        io.print(\"yes\")\n    } else {\n        io.print(\"no\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile if bool");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run if bool");
        assert_eq!(result.output, "yes");
    }

    #[test]
    fn test_compile_with_object() {
        let source = "fn main() {\n    let obj = {name: \"crush\", version: 42}\n    io.print(obj.name)\n}\n";
        let prog = compile_crush_source(source).expect("compile object");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run object");
        assert_eq!(result.output, "crush");
    }

    #[test]
    fn test_compile_with_try_catch() {
        let source = "fn main() {\n    try {\n        io.print(\"in try\")\n    } catch err {\n        io.print(\"in catch\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile try/catch");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run try/catch");
        assert_eq!(result.output, "in try");
    }

    #[test]
    fn test_compile_with_throw_and_catch() {
        let source = "fn main() {\n    try {\n        throw \"error!\"\n        io.print(\"not reached\")\n    } catch err {\n        io.print(\"caught\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile throw/catch");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run throw/catch");
        assert_eq!(result.output, "caught");
    }
}
