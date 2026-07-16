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
    let mut program = crush_frontend::parse_source(source)?;
    prepare_polyglot_blocks(&mut program);
    let casm_program = crush_frontend::compile_cast(&program)?;
    casm_to_vm(&casm_program)
}

/// Fill in `Statement::LangBlock.variables` (inputs) and
/// `meta["polyglot_output"]` (the single output var, per the current
/// exec_lang protocol) for every `@python { ... }` block, via real
/// free-variable analysis over the block's own AST — never a regex, never
/// a blind "inject everything in scope". See
/// `crush_lang_python::analyzer::free_variables`.
///
/// Other languages (no parser wired yet) are left alone: their blocks
/// compile with no marshaling, the same behavior as before this pass
/// existed — not a regression, just not-yet-implemented. A malformed
/// Python block is left unmarshaled too rather than failing Crush
/// compilation outright; the actual `python3` subprocess will raise its
/// own loud syntax error at run time, which is still honest, just later.
fn prepare_polyglot_blocks(program: &mut crush_cast::Program) {
    for func in program.functions.values_mut() {
        let mut known_locals: HashSet<String> =
            func.params.iter().map(|(name, _)| name.clone()).collect();
        prepare_stmts(&mut func.body, &mut known_locals);
    }
}

fn prepare_stmts(stmts: &mut [crush_cast::Statement], known_locals: &mut HashSet<String>) {
    use crush_cast::Statement;
    for stmt in stmts.iter_mut() {
        match stmt {
            Statement::VarDecl { name, .. }
            | Statement::Assign { target: name, .. }
            | Statement::Export { name, .. } => {
                known_locals.insert(name.clone());
            }
            Statement::LangBlock {
                lang,
                code,
                variables,
                meta,
                ..
            } if lang == "python" => {
                // The lexer captures the `{ ... }` body verbatim, including
                // whatever leading indentation it happened to sit at inside
                // `@python { ... }`. `python3 -c` tolerates that on its own
                // (indentation is only ever relative), but
                // rustpython-parser's `Suite::parse` here does not — dedent
                // first so analysis sees the same shape `python3` would
                // execute. The real `code` sent to the subprocess is
                // dedented too, but only if a prologue/epilogue actually
                // gets spliced in below (see `rewrite_python_marshaling`);
                // an unmarshaled block is left completely untouched.
                #[cfg(feature = "polyglot-python")]
                if let Ok(free_vars) = crush_lang_python::analyzer::free_variables(&dedent(code)) {
                    let inputs: Vec<String> = free_vars
                        .reads
                        .into_iter()
                        .filter(|name| known_locals.contains(name))
                        .collect();
                    let output_var = free_vars.top_level_bound.last().cloned();
                    if let Some(output_var) = &output_var {
                        meta.insert(
                            "polyglot_output".to_string(),
                            serde_json::json!(output_var),
                        );
                        known_locals.insert(output_var.clone());
                    }
                    if !inputs.is_empty() || output_var.is_some() {
                        *code = rewrite_python_marshaling(code, &inputs, output_var.as_deref());
                    }
                    *variables = inputs;
                }
                // Without `polyglot-python` (e.g. crush-web's wasm32 build,
                // where EXEC_LANG can't spawn a subprocess anyway — see
                // portable_vm.rs/scheduler.rs), the block is left unmarshaled,
                // same as any other not-yet-wired language: not a regression,
                // just not applicable when polyglot execution itself is
                // unavailable on this target.
                #[cfg(not(feature = "polyglot-python"))]
                let _ = (lang, code, variables, meta, &known_locals);
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                prepare_stmts(then_body, known_locals);
                if let Some(else_body) = else_body {
                    prepare_stmts(else_body, known_locals);
                }
            }
            Statement::While { body, .. } => prepare_stmts(body, known_locals),
            Statement::For {
                variable, body, ..
            } => {
                known_locals.insert(variable.clone());
                prepare_stmts(body, known_locals);
            }
            _ => {}
        }
    }
}

/// Rewrite a `@python` block's source to bridge the Crush/Python value
/// boundary: `scheduler.rs`'s EXEC_LANG only gives the subprocess its
/// inputs as OS environment variables (always strings) and only gets a
/// value back by scanning stdout for `crush_vm::scheduler::CRUSH_RESULT_SENTINEL`
/// — the block's own bound names mean nothing to either side of that
/// boundary on their own.
///
/// - Inputs: JSON-decoded from `os.environ` into real Python locals
///   (round-trips int/float/bool/str/list/map, not just strings).
/// - Output: JSON-encoded and printed on a sentinel-prefixed line, kept
///   distinct from the block's own ordinary prints (which still flow
///   through as normal program output — stdout is a shared channel).
/// - A value that can't be JSON-encoded (NaN, a set, a custom object)
///   fails loudly, naming the variable and its type, rather than being
///   silently dropped.
///
/// The whole input-decoding prologue is kept on a single physical line so
/// it shifts every line of the user's own code by exactly one line,
/// keeping Python traceback line numbers predictable rather than off by
/// a count that depends on how many variables happen to be marshaled.
#[cfg(feature = "polyglot-python")]
fn rewrite_python_marshaling(code: &str, inputs: &[String], output_var: Option<&str>) -> String {
    let mut prologue_parts: Vec<String> = Vec::new();
    if inputs.is_empty() {
        prologue_parts.push("import json as __crush_json".to_string());
    } else {
        prologue_parts.push("import json as __crush_json, os as __crush_os".to_string());
        for name in inputs {
            prologue_parts.push(format!(
                "{name} = __crush_json.loads(__crush_os.environ[{name:?}])"
            ));
        }
    }
    let prologue = prologue_parts.join("; ");

    // Deliberately hand-written as a literal Python string token — NOT
    // derived from `CRUSH_RESULT_SENTINEL` via `{:?}` — because Rust's
    // Debug escaping (`\0`) and Python's string-escape syntax (`\x00`)
    // are different grammars that happen to overlap for this one
    // character; relying on that coincidence would be fragile. This
    // MUST stay byte-for-byte in sync with
    // `crush_vm::scheduler::CRUSH_RESULT_SENTINEL` by hand.
    const PY_SENTINEL_LITERAL: &str = "\"\\x00CRUSH_RESULT\\x00\"";

    let epilogue = match output_var {
        // `name` is always a plain identifier (came off the AST as a bound
        // name), never containing a quote — safe to splice directly into
        // a single-quoted Python string literal without escaping.
        Some(name) => format!(
            "\ntry:\n    print({PY_SENTINEL_LITERAL} + __crush_json.dumps({name}))\nexcept TypeError as __crush_marshal_err:\n    import sys as __crush_sys\n    print(\"cannot marshal output variable '{name}' (type \" + type({name}).__name__ + \"): \" + str(__crush_marshal_err), file=__crush_sys.stderr)\n    __crush_sys.exit(1)\n",
        ),
        None => String::new(),
    };

    // The block's own leading indentation (from however it sits inside
    // `@python { ... }`) is harmless on its own — `python3 -c` accepts a
    // uniformly-indented script, since indentation is only ever relative.
    // It becomes a real `IndentationError` the moment a column-0 prologue
    // line precedes it, so dedent first to keep the two consistent.
    format!("{prologue}\n{}{epilogue}", dedent(code))
}

/// Strip the common leading whitespace shared by every non-blank line, so
/// a block's original relative indentation is preserved but its absolute
/// column no longer depends on how it happened to sit inside `{ ... }`.
#[cfg(feature = "polyglot-python")]
fn dedent(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let common_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(common_indent..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn casm_to_vm(program: &casm::Program) -> anyhow::Result<crush_vm::Program> {
    let mut lines: Vec<String> = Vec::new();
    let mut perms: HashSet<String> = HashSet::new();

    for _func in program.functions.values() {
        perms.extend(program.manifest.permissions.clone());
    }

    for (fname, func) in &program.functions {
        let mut slot_map: HashMap<String, u16> = HashMap::new();
        let mut next_slot: u16 = 0;
        let local_funcs: HashSet<String> = program.functions.keys().cloned().collect();

        lines.push(format!(".func {fname}"));

        let mut target_labels: HashMap<usize, String> = HashMap::new();
        for instr in func.body.iter() {
            if (instr.op == "jmp"
                || instr.op == "jmp_if"
                || instr.op == "jmp_if_not"
                || instr.op == "enter_try")
                && let Some(target) = instr.args.get("target").and_then(|v| v.as_u64())
            {
                target_labels
                    .entry(target as usize)
                    .or_insert_with(unique_label);
            }
        }

        for (i, instr) in func.body.iter().enumerate() {
            if let Some(label) = target_labels.get(&i) {
                lines.push(format!("{label}:"));
            }

            let op = match instr.op.as_str() {
                "push_int" => {
                    let v = instr.args["value"]
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("push_int missing value at {fname}:{i}"))?;
                    format!("PUSH {v}")
                }
                "push_float" => {
                    let v = instr.args["value"].as_f64().ok_or_else(|| {
                        anyhow::anyhow!("push_float missing value at {fname}:{i}")
                    })?;
                    format!("PUSH_F64 {v}")
                }
                "push_str" => {
                    let v = instr.args["value"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("push_str missing value at {fname}:{i}"))?;
                    format!("PUSH_STR {v:?}")
                }
                "push_bool" => {
                    let v = instr.args["value"]
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("push_bool missing value at {fname}:{i}"))?;
                    format!("PUSH_BOOL {}", if v { 1 } else { 0 })
                }
                "push_null" => "PUSH_NULL".to_string(),
                "pop" => "POP".to_string(),
                "dup" => "DUP".to_string(),
                "load" => {
                    let name = instr.args["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("load missing name at {fname}:{i}"))?;
                    let slot = get_slot(name, &mut slot_map, &mut next_slot);
                    format!("LOAD {slot}")
                }
                "store" => {
                    let name = instr.args["name"]
                        .as_str()
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
                "ne" => "NE".to_string(),
                "lt" => "LT".to_string(),
                "gt" => "GT".to_string(),
                "le" => "LE".to_string(),
                "ge" => "GE".to_string(),
                "not" => "NOT".to_string(),
                "neg" => "NEG".to_string(),
                "and" => "AND".to_string(),
                "or" => "OR".to_string(),
                "call" => {
                    let fn_name = instr.args["function"]
                        .as_str()
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
                    let name = instr.args["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("cap_call missing name at {fname}:{i}"))?;
                    let argc = instr.args["argc"].as_u64().unwrap_or(0);
                    perms.insert(name.to_string());
                    format!("CAP_CALL {name:?} {argc}")
                }
                "ret" => "RET".to_string(),
                "jmp" => {
                    let target = instr.args["target"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("jmp missing target at {fname}:{i}"))?
                        as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("jmp to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("JMP {label}")
                }
                "jmp_if_not" => {
                    let target = instr.args["target"].as_u64().ok_or_else(|| {
                        anyhow::anyhow!("jmp_if_not missing target at {fname}:{i}")
                    })? as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("jmp_if_not to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("JZ {label}")
                }
                "jmp_if" => {
                    let target = instr.args["target"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("jmp_if missing target at {fname}:{i}"))?
                        as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("jmp_if to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("JNZ {label}")
                }
                "new_array" => {
                    let size = instr.args.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    "NEW_ARRAY 0".to_string()
                }
                "array_push" => "ARR_PUSH".to_string(),
                "array_pop" => "ARR_POP".to_string(),
                "len" => "ARR_LEN".to_string(),
                "index" | "arr_get" => "ARR_GET".to_string(),
                "make_range" => "MAKE_RANGE".to_string(),
                "str_contains" => "STR_CONTAINS".to_string(),
                "str_split" => "STR_SPLIT".to_string(),
                "str_replace" => "STR_REPLACE".to_string(),
                "str_join" => "STR_JOIN".to_string(),
                "arr_set" => "ARR_SET".to_string(),
                "export_var" => "NOP".to_string(),
                "exec_lang" => {
                    let args_json = serde_json::to_string(&instr.args).map_err(|e| {
                        anyhow::anyhow!("exec_lang: failed to serialize args at {fname}:{i}: {e}")
                    })?;
                    // Escape for assembly: \ → \\, " → \"
                    let esc = args_json.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("EXEC_LANG \"{esc}\"")
                }
                "spawn" => "SPAWN".to_string(),
                "yield" => "YIELD".to_string(),
                "await" => "AWAIT".to_string(),
                "throw" => "THROW".to_string(),
                "enter_try" => {
                    let target = instr.args["target"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("enter_try missing target at {fname}:{i}"))?
                        as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("enter_try to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("ENTER_TRY {label}")
                }
                "halt" => "HALT".to_string(),
                "exit_try" => "EXIT_TRY".to_string(),
                "new_obj" => "NEW_OBJ".to_string(),
                "get_field" => {
                    let field = instr
                        .args
                        .get("field")
                        .or_else(|| instr.args.get("name"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("get_field missing field arg at {fname}:{i}")
                        })?;
                    format!("GET_FIELD {field:?}")
                }
                "set_field" => {
                    let field = instr
                        .args
                        .get("field")
                        .or_else(|| instr.args.get("name"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("set_field missing field arg at {fname}:{i}")
                        })?;
                    format!("SET_FIELD {field:?}")
                }
                // Stubs for unimplemented opcodes — prevent bailing
                "new_struct" => "NEW_OBJ".to_string(),
                "dom_mutate" | "dom_event_listener" | "dom_query" => "NOP".to_string(),
                "ai_goal_decl" | "ai_progress_update" | "ai_knowledge_share" => "NOP".to_string(),
                "ai_capability_discovery" | "ai_adaptation_request" => "NOP".to_string(),
                "ai_query" | "ai_tool_chain" | "ai_agent_delegation" => "NOP".to_string(),
                "ai_learning_loop" | "ai_context_aware" => "NOP".to_string(),
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
            if let Some(suppress) = suppress_next_pop.take()
                && suppress
            {
                continue;
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
        if perms_slice.is_empty() {
            None
        } else {
            Some(&perms_slice)
        },
        None,
    )?;
    Ok(vm_program)
}

#[cfg(test)]
mod tests {
    fn _poly_caps() -> crush_vm::HostCaps {
        let mut c = crush_vm::HostCaps::new();
        c.grant_polyglot(&["python", "javascript", "bash"]);
        c
    }
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
        assert_eq!(result.output, "hello from crush\n");
        assert!(result.halted);
    }

    #[test]
    fn test_compile_with_bool() {
        let source = "fn main() {\n    let a = true\n    let b = false\n    io.print(a)\n}\n";
        let prog = compile_crush_source(source).expect("compile bool");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run bool");
        assert_eq!(result.output, "true\n");
    }

    #[test]
    fn test_compile_with_if_bool_condition() {
        let source = "fn main() {\n    if true {\n        io.print(\"yes\")\n    } else {\n        io.print(\"no\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile if bool");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run if bool");
        assert_eq!(result.output, "yes\n");
    }

    #[test]
    fn test_compile_with_object() {
        let source = "fn main() {\n    let obj = {name: \"crush\", version: 42}\n    io.print(obj.name)\n}\n";
        let prog = compile_crush_source(source).expect("compile object");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run object");
        assert_eq!(result.output, "crush\n");
    }

    #[test]
    fn test_compile_with_try_catch() {
        let source = "fn main() {\n    try {\n        io.print(\"in try\")\n    } catch err {\n        io.print(\"in catch\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile try/catch");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run try/catch");
        assert_eq!(result.output, "in try\n");
    }

    #[test]
    fn test_compile_with_throw_and_catch() {
        let source = "fn main() {\n    try {\n        throw \"error!\"\n        io.print(\"not reached\")\n    } catch err {\n        io.print(\"caught\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile throw/catch");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run throw/catch");
        assert_eq!(result.output, "caught\n");
    }

    // Regression: a `@javascript { ... }` polyglot block used to fail at
    // CASM assembly time (the source's `;` broke the assembler's comment
    // stripper — see crush_vm::assembler tests) before ever reaching the
    // language executor, and even once it parsed, EXEC_LANG ran `node -c`
    // (syntax-check-only) instead of `node -e` (execute). Requires `node`
    // on PATH.
    #[test]
    fn test_javascript_polyglot_block_executes() {
        let source = "fn main() {\n    @javascript { const x = 1; }\n    io.print(\"js ok\")\n}\n";
        let prog = compile_crush_source(source).expect("compile js polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run js polyglot block");
        assert_eq!(result.output, "js ok\n");
    }

    // An `@<lang>` block for a language with no registered executor must
    // fail loudly and name the language — never silently no-op.
    #[test]
    fn test_unregistered_polyglot_language_errors_loudly() {
        let source = "fn main() {\n    @cobol { DISPLAY \"hi\". }\n}\n";
        let prog = compile_crush_source(source).expect("compile unregistered-lang block");
        let quotas = crush_vm::Quotas::default();
        let err = crush_vm::run_with_caps(&prog, &quotas, None)
            .expect_err("unregistered language must not silently succeed");
        let msg = err.to_string();
        assert!(
            msg.contains("no executor registered for language 'cobol'"),
            "error should name the unregistered language, got: {msg}"
        );
    }

    // A hung/slow polyglot subprocess must be killed at
    // `Quotas::max_wall_time_ms`, not allowed to block the interpreter
    // indefinitely — `max_steps` never trips for a process blocked on I/O,
    // since it isn't executing crush instructions while it hangs. The real
    // proof isn't just the error variant; it's that this test finishes in
    // well under the process's 30s sleep, showing the subprocess was
    // actually killed rather than eventually finishing on its own.
    #[test]
    fn test_exec_lang_wall_clock_timeout_kills_hung_subprocess() {
        let source = "fn main() {\n    @bash {\n        sleep 30\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile bash polyglot block");
        let quotas = crush_vm::Quotas {
            max_wall_time_ms: 200,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let err = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect_err("a hung subprocess must be killed, not silently allowed to keep running");
        let elapsed = start.elapsed();
        let msg = err.to_string();
        assert!(
            msg.contains("wall-clock quota") && msg.contains("killed") && msg.contains("polyglot.bash"),
            "error should name the capability and say it was killed, got: {msg}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "the 30s sleep must be KILLED at the 200ms quota, not waited out — took {elapsed:?}"
        );
    }

    // End-to-end: a Crush local flows into a `@python` block (input
    // marshaling) and the block's own bound name flows back out (output
    // marshaling), entirely via the AST free-variable analysis — no
    // explicit declaration syntax needed. Requires `python3` on PATH.
    #[test]
    fn test_python_polyglot_block_marshals_input_and_output() {
        let source =
            "fn main() {\n    let base = 5;\n    @python {\n        result = base * 2\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run python polyglot block");
        assert_eq!(result.output, "10\n");
    }

    // Same, but multi-line with an import and a non-integer result — the
    // real shape of crush-website's example.crush.
    #[test]
    fn test_python_polyglot_block_marshals_float_result_with_import() {
        let source = "fn main() {\n    let base = 5;\n    @python {\n        import math\n        result = math.pow(base, 3)\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run python polyglot block");
        // JSON round-trip preserves float-ness (Python's json.dumps(125.0)
        // stays "125.0"), not just the numeric value.
        assert_eq!(result.output, "125.0\n");
    }

    // The block's own ordinary prints must keep flowing through as normal
    // program output — only the sentinel-marked line is the marshaled
    // return value, never the whole stdout stream.
    #[test]
    fn test_python_polyglot_block_own_prints_still_visible() {
        let source = "fn main() {\n    let base = 5;\n    @python {\n        print(\"debug\")\n        result = base + 1\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run python polyglot block");
        assert!(
            result.output.contains("debug"),
            "block's own print output should still appear, got: {}",
            result.output
        );
        assert!(
            result.output.contains('6'),
            "marshaled result should still appear, got: {}",
            result.output
        );
    }

    #[test]
    fn test_dedent_strips_common_indent_only() {
        assert_eq!(dedent("    a\n    b\n"), "a\nb");
        assert_eq!(dedent("  a\n    b\n"), "a\n  b");
        assert_eq!(dedent("a"), "a");
    }
}
