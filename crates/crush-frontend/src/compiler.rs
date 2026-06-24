use anyhow::{Result, bail};
use casm::debug_info::{DebugInfo, SourceLocation};
use casm::{Function as CasmFunction, Instruction, Manifest, Program as CasmProgram};
use crush_cast::*;
use std::collections::{HashMap, HashSet};

pub type CompileError = anyhow::Error;

pub struct Compiler {
    all_permissions: HashSet<String>,
    temp_counter: usize,
    loop_stack: Vec<LoopInfo>,
    last_debug_info: Option<DebugInfo>,
    local_functions: HashSet<String>,
    /// When true, every function whose name appears in an `Invariant`'s
    /// `applies_to` list AND that has a non-empty `check_source` gets a
    /// `cap_call "invariant.evaluate"` instruction prepended, AFTER the
    /// parameter stores (so the call may read function arguments). Off by
    /// default; toggle via [`Compiler::with_invariant_runtime`].
    invariant_runtime: bool,
    lambdas: HashMap<String, CasmFunction>,
}

struct LoopInfo {
    continue_target: usize,
    break_indices: Vec<usize>,
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            all_permissions: HashSet::new(),
            temp_counter: 0,
            loop_stack: Vec::new(),
            last_debug_info: None,
            local_functions: HashSet::new(),
            invariant_runtime: false,
            lambdas: HashMap::new(),
        }
    }

    /// Builder: enable (`true`) or disable (`false`) emission of
    /// `cap_call "invariant.evaluate"` instructions for matching
    /// `@invariant` blocks. Default: `false` (compiler is an offline tool;
    /// runtime invariant evaluation is opt-in to avoid surprising existing
    /// callers).
    pub fn with_invariant_runtime(mut self, enabled: bool) -> Self {
        self.invariant_runtime = enabled;
        self
    }

    pub fn compile(&mut self, mut program: Program) -> Result<CasmProgram> {
        self.local_functions.clear();
        self.lambdas.clear();
        for name in program.functions.keys() {
            self.local_functions.insert(name.clone());
        }
        // Pre-pass: track VarDecls and populate LangBlock variables
        for func in program.functions.values_mut() {
            let mut declared: Vec<String> = Vec::new();
            for stmt in &mut func.body {
                match stmt {
                    Statement::VarDecl { name, .. } => {
                        declared.push(name.clone());
                    }
                    Statement::FunctionDef { name, .. } => {
                        self.local_functions.insert(name.clone());
                    }
                    Statement::LangBlock { variables, .. } if variables.is_empty() => {
                        *variables = declared.clone();
                    }
                    _ => {}
                }
            }
        }

        let mut debug_info = DebugInfo::new();
        let mut source_files: HashMap<String, usize> = HashMap::new();
        let mut casm_program = CasmProgram {
            version: program.cast_version,
            functions: HashMap::new(),
            lang: program.lang,
            manifest: Manifest::default(),
        };

        for (name, func) in program.functions {
            let mut instrs = Vec::new();

            // First pass: Handle nested FunctionDef nodes
            for stmt in &func.body {
                if let Statement::FunctionDef {
                    name,
                    params,
                    body,
                    meta,
                } = stmt
                {
                    let mut func_instrs = Vec::new();
                    for (param_name, _) in params {
                        func_instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": param_name}),
                            meta,
                        ));
                    }
                    for inner_stmt in body {
                        self.compile_stmt(inner_stmt, &mut func_instrs)?;
                    }
                    self.ensure_return(&mut func_instrs, Some(meta));
                    self.record_debug_info_for_function(
                        name,
                        &func_instrs,
                        &mut debug_info,
                        &mut source_files,
                    );

                    casm_program.functions.insert(
                        name.clone(),
                        CasmFunction {
                            params: params.iter().map(|(n, _)| n.clone()).collect(),
                            locals: vec![],
                            body: func_instrs,
                        },
                    );
                }
            }

            // Second pass: Compile main function instructions
            for (param_name, _) in &func.params {
                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": param_name}),
                    &func.meta,
                ));
            }

            // Optionally emit `cap_call "invariant.evaluate"` for every
            // `@invariant` that targets this function name AND carries a
            // non-empty `check_source`. Toggle on via
            // `Compiler::with_invariant_runtime(true)`. Emitted *after* the
            // param-store loop so the runtime evaluator may read function
            // arguments from the operand stack if the check expression
            // references them.
            if self.invariant_runtime {
                if let Some(manifest) = &program.manifest {
                    for inv in &manifest.invariants {
                        if !inv.applies_to.contains(&name) {
                            continue;
                        }
                        let Some(src) = inv.check_source.as_deref() else {
                            continue;
                        };
                        // Empty `check_source` is a doc stub — no evaluator
                        // can run an empty expression. Asking the runtime to
                        // fetch a cap for it would just produce an unhelpful
                        // cap_error, so skip silently (same policy as the
                        // missing-source case immediately above).
                        if src.is_empty() {
                            continue;
                        }
                        self.all_permissions.insert("invariant.evaluate".to_string());

                        let args = serde_json::json!({
                            "name": "invariant.evaluate",
                            "argc": 0,
                            "invariant_name": inv.name.clone(),
                            "function_name": name.clone(),
                            "check_source": src,
                        });
                        instrs.push(self.create_instr("cap_call", args, &func.meta));
                    }
                }
            }
            for stmt in &func.body {
                if !matches!(stmt, Statement::FunctionDef { .. }) {
                    self.compile_stmt(stmt, &mut instrs)?;
                }
            }

            self.ensure_return(&mut instrs, Some(&func.meta));
            self.record_debug_info_for_function(&name, &instrs, &mut debug_info, &mut source_files);

            casm_program.functions.insert(
                name,
                CasmFunction {
                    params: func.params.iter().map(|(n, _)| n.clone()).collect(),
                    locals: vec![],
                    body: instrs,
                },
            );
        }

        // Merge lambdas
        for (name, func) in self.lambdas.drain() {
            casm_program.functions.insert(name, func);
        }

        if !self.all_permissions.is_empty() {
            casm_program.manifest = Manifest {
                permissions: self.all_permissions.iter().cloned().collect(),
            };
        }

        self.last_debug_info = Some(debug_info);

        Ok(casm_program)
    }

    pub fn debug_info(&self) -> Option<&DebugInfo> {
        self.last_debug_info.as_ref()
    }

    fn record_debug_info_for_function(
        &self,
        function_name: &str,
        instrs: &[Instruction],
        debug_info: &mut DebugInfo,
        source_files: &mut HashMap<String, usize>,
    ) {
        for (pc, instr) in instrs.iter().enumerate() {
            let loc = instr
                .meta
                .as_ref()
                .and_then(casm::debug_info::extract_source_location)
                .unwrap_or_else(|| SourceLocation::new(1, 1, None));

            debug_info.push_source_location(loc.clone());

            let file_idx = if let Some(file) = &loc.file {
                if let Some(idx) = source_files.get(file) {
                    *idx
                } else {
                    let idx = debug_info.add_source(file.clone(), Some("crush".to_string()));
                    source_files.insert(file.clone(), idx);
                    idx
                }
            } else if let Some(idx) = source_files.get("<unknown>") {
                *idx
            } else {
                let idx = debug_info.add_source("<unknown>", Some("crush".to_string()));
                source_files.insert("<unknown>".to_string(), idx);
                idx
            };

            debug_info.map_instruction(function_name, pc, file_idx, loc.line, loc.col);
        }
    }

    fn ensure_return(
        &self,
        instrs: &mut Vec<Instruction>,
        meta: Option<&HashMap<String, serde_json::Value>>,
    ) {
        if !instrs.iter().any(|i| i.op == "ret") {
            let meta_json = meta
                .map(|m| serde_json::to_value(m).unwrap())
                .unwrap_or(serde_json::json!({}));
            let lang = meta
                .and_then(|m| m.get("lang"))
                .and_then(|l| l.as_str())
                .map(|s| s.to_string());

            instrs.push(Instruction {
                op: "push_null".to_string(),
                args: serde_json::json!({}),
                lang: lang.clone(),
                meta: Some(meta_json.clone()),
            });
            instrs.push(Instruction {
                op: "ret".to_string(),
                args: serde_json::json!({}),
                lang,
                meta: Some(meta_json),
            });
        }
    }

    fn compile_pattern(
        &mut self,
        pattern: &Pattern,
        matched_val: &str,
        instrs: &mut Vec<Instruction>,
        fail_jumps: &mut Vec<usize>,
    ) -> Result<()> {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Literal { value } => {
                self.compile_expr(value, instrs)?;
                instrs.push(self.create_instr(
                    "load",
                    serde_json::json!({"name": matched_val}),
                    &HashMap::new(),
                ));
                instrs.push(self.create_instr("eq", serde_json::json!({}), &HashMap::new()));
                fail_jumps.push(instrs.len());
                instrs.push(self.create_instr(
                    "jmp_if_not",
                    serde_json::json!({"target": 0}),
                    &HashMap::new(),
                ));
            }
            Pattern::Identifier { name } => {
                instrs.push(self.create_instr(
                    "load",
                    serde_json::json!({"name": matched_val}),
                    &HashMap::new(),
                ));
                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": name}),
                    &HashMap::new(),
                ));
            }
            Pattern::Struct { name: _, fields } => {
                for (field_name, sub_pattern) in fields {
                    instrs.push(self.create_instr(
                        "load",
                        serde_json::json!({"name": matched_val}),
                        &HashMap::new(),
                    ));
                    instrs.push(self.create_instr(
                        "get_field",
                        serde_json::json!({"field": field_name}),
                        &HashMap::new(),
                    ));
                    let sub_temp = format!("__match_sub_{}_{}", field_name, self.temp_counter);
                    self.temp_counter += 1;
                    instrs.push(self.create_instr(
                        "store",
                        serde_json::json!({"name": sub_temp}),
                        &HashMap::new(),
                    ));
                    self.compile_pattern(sub_pattern, &sub_temp, instrs, fail_jumps)?;
                }
            }
        }
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Statement, instrs: &mut Vec<Instruction>) -> Result<()> {
        match stmt {
            Statement::VarDecl {
                name, value, meta, ..
            } => {
                self.compile_expr_with_name_hint(value, instrs, Some(name))?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": name}), meta));
            }
            Statement::Export { name, value, meta } => {
                self.compile_expr(value, instrs)?;
                instrs.push(self.create_instr(
                    "export_var",
                    serde_json::json!({"name": name}),
                    meta,
                ));
            }
            Statement::ExprStmt { expr, meta } => {
                self.compile_expr(expr, instrs)?;
                instrs.push(self.create_instr("pop", serde_json::json!({}), meta));
            }
            Statement::Return { value, meta } => {
                if let Some(val) = value {
                    self.compile_expr(val, instrs)?;
                } else {
                    instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
                }
                instrs.push(self.create_instr("ret", serde_json::json!({}), meta));
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                meta,
            } => {
                self.compile_expr(condition, instrs)?;
                let jmp_if_not_idx = instrs.len();
                instrs.push(self.create_instr(
                    "jmp_if_not",
                    serde_json::json!({"target": 0}),
                    meta,
                ));

                for s in then_body {
                    self.compile_stmt(s, instrs)?;
                }

                let jmp_end_idx = if else_body.is_some() {
                    let idx = instrs.len();
                    instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), meta));
                    Some(idx)
                } else {
                    None
                };

                let else_start = instrs.len();
                instrs[jmp_if_not_idx].args = serde_json::json!({"target": else_start});

                if let Some(eb) = else_body {
                    for s in eb {
                        self.compile_stmt(s, instrs)?;
                    }
                }

                if let Some(idx) = jmp_end_idx {
                    let end_label = instrs.len();
                    instrs[idx].args = serde_json::json!({"target": end_label});
                }
            }
            Statement::While {
                condition,
                body,
                meta,
            } => {
                let loop_start = instrs.len();
                self.loop_stack.push(LoopInfo {
                    continue_target: loop_start,
                    break_indices: Vec::new(),
                });

                self.compile_expr(condition, instrs)?;
                let jmp_if_not_idx = instrs.len();
                instrs.push(self.create_instr(
                    "jmp_if_not",
                    serde_json::json!({"target": 0}),
                    meta,
                ));

                for s in body {
                    self.compile_stmt(s, instrs)?;
                }

                instrs.push(self.create_instr(
                    "jmp",
                    serde_json::json!({"target": loop_start}),
                    meta,
                ));
                let loop_end = instrs.len();
                instrs[jmp_if_not_idx].args = serde_json::json!({"target": loop_end});

                // Patch break jumps
                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_indices {
                    instrs[idx].args = serde_json::json!({"target": loop_end});
                }
            }
            Statement::For {
                variable,
                iterable,
                body,
                meta,
            } => {
                // Minimal implementation: desugar to while loop with index
                // for item in array { body }
                // becomes:
                // let __arr = array
                // let __i = 0
                // while __i < len(__arr) {
                //     let item = __arr[__i]
                //     body
                //     __i = __i + 1
                // }

                let arr_var = format!("__arr_{}", self.temp_counter);
                let idx_var = format!("__i_{}", self.temp_counter);
                self.temp_counter += 1;

                // Compile iterable and store in temp var
                self.compile_expr(iterable, instrs)?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": arr_var}), meta));

                // Initialize index to 0
                instrs.push(self.create_instr("push_int", serde_json::json!({"value": 0}), meta));
                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": &idx_var}),
                    meta,
                ));

                // Loop start
                let loop_start = instrs.len();
                // continue_target will be updated later (before increment)
                // Actually, in many languages, continue jumps to the increment part.
                // Let's check where the increment is. It's at the end.
                // So if we continue, we skip the increment? No, that would be an infinite loop.
                // So continue should jump to the increment part.

                // Let's place a label for continue before the increment.

                self.loop_stack.push(LoopInfo {
                    continue_target: 0, // Will patch later or adjust logic
                    break_indices: Vec::new(),
                });

                // Load array and get length (push array, call len)
                instrs.push(self.create_instr("load", serde_json::json!({"name": &arr_var}), meta));
                instrs.push(self.create_instr("len", serde_json::json!({}), meta));

                // Load index
                instrs.push(self.create_instr("load", serde_json::json!({"name": &idx_var}), meta));

                // Compare: length > index (since we pushed length first)
                instrs.push(self.create_instr("gt", serde_json::json!({}), meta));

                // Jump if not (exit loop)
                let jmp_if_not_idx = instrs.len();
                instrs.push(self.create_instr(
                    "jmp_if_not",
                    serde_json::json!({"target": 0}),
                    meta,
                ));

                // Get array[index] and store in loop variable
                instrs.push(self.create_instr("load", serde_json::json!({"name": &arr_var}), meta));
                instrs.push(self.create_instr("load", serde_json::json!({"name": &idx_var}), meta));
                instrs.push(self.create_instr("index", serde_json::json!({}), meta));
                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": variable}),
                    meta,
                ));

                // Compile body
                for s in body {
                    self.compile_stmt(s, instrs)?;
                }

                // Continue target is here (before increment)
                let continue_idx = instrs.len();
                self.loop_stack.last_mut().unwrap().continue_target = continue_idx;

                // Increment index: __i = __i + 1
                instrs.push(self.create_instr("load", serde_json::json!({"name": &idx_var}), meta));
                instrs.push(self.create_instr("push_int", serde_json::json!({"value": 1}), meta));
                instrs.push(self.create_instr("add", serde_json::json!({}), meta));
                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": &idx_var}),
                    meta,
                ));

                // Jump back to loop start
                instrs.push(self.create_instr(
                    "jmp",
                    serde_json::json!({"target": loop_start}),
                    meta,
                ));

                // Loop end
                let loop_end = instrs.len();
                instrs[jmp_if_not_idx].args = serde_json::json!({"target": loop_end});

                // Patch break jumps
                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_indices {
                    instrs[idx].args = serde_json::json!({"target": loop_end});
                }
            }
            Statement::FunctionDef { .. } => {} // Handled in first pass
            Statement::SetField {
                target,
                field,
                value,
                meta,
            } => {
                self.compile_expr(target, instrs)?;
                self.compile_expr(value, instrs)?;
                instrs.push(self.create_instr(
                    "set_field",
                    serde_json::json!({"name": field}),
                    meta,
                ));
            }
            Statement::Throw { value, meta } => {
                self.compile_expr(value, instrs)?;
                instrs.push(self.create_instr("throw", serde_json::json!({}), meta));
            }
            Statement::TryCatch {
                body,
                error_var,
                handler,
                meta,
            } => {
                let enter_try_idx = instrs.len();
                instrs.push(self.create_instr("enter_try", serde_json::json!({"target": 0}), meta));

                for s in body {
                    self.compile_stmt(s, instrs)?;
                }

                instrs.push(self.create_instr("exit_try", serde_json::json!({}), meta));

                let jmp_end_idx = instrs.len();
                instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), meta));

                let handler_start = instrs.len();
                instrs[enter_try_idx].args = serde_json::json!({"target": handler_start});

                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": error_var}),
                    meta,
                ));

                for s in handler {
                    self.compile_stmt(s, instrs)?;
                }

                let end_label = instrs.len();
                instrs[jmp_end_idx].args = serde_json::json!({"target": end_label});
            }
            Statement::LangBlock {
                lang,
                code,
                variables,
                imports: _, // Ignore for now
                meta,
            } => {
                // Push variables onto stack for injection into the sandbox
                for var_name in variables {
                    instrs.push(self.create_instr(
                        "load",
                        serde_json::json!({"name": var_name}),
                        meta,
                    ));
                }

                // Build variable name mapping for the exec_lang instruction
                let mut var_args = serde_json::json!({
                    "lang": lang,
                    "code": code,
                    "var_count": variables.len()
                });

                // Add variable names as var_0, var_1, etc.
                if let serde_json::Value::Object(ref mut map) = var_args {
                    for (i, var_name) in variables.iter().enumerate() {
                        map.insert(format!("var_{}", i), serde_json::json!(var_name));
                    }
                }

                instrs.push(self.create_instr("exec_lang", var_args, meta));

                // Store output back to the first variable (if any)
                if let Some(first_var) = variables.first() {
                    instrs.push(self.create_instr(
                        "store",
                        serde_json::json!({"name": first_var}),
                        meta,
                    ));
                }
            }
            Statement::Break { meta } => {
                if !self.loop_stack.is_empty() {
                    let idx = instrs.len();
                    let instr = self.create_instr("jmp", serde_json::json!({"target": 0}), meta);
                    instrs.push(instr);
                    let loop_info = self.loop_stack.last_mut().unwrap();
                    loop_info.break_indices.push(idx);
                } else {
                    bail!("break outside of loop");
                }
            }
            Statement::Continue { meta } => {
                if let Some(loop_info) = self.loop_stack.last() {
                    instrs.push(self.create_instr(
                        "jmp",
                        serde_json::json!({"target": loop_info.continue_target}),
                        meta,
                    ));
                } else {
                    bail!("continue outside of loop");
                }
            }
            Statement::StructDef { .. } => {}
            Statement::Import { import, meta } => {
                // Compile import statements to capability/module load instructions.
                // The ImportResolver handles resolution at runtime via capabilities.
                match import {
                    ImportStatement::CrushModule {
                        module_path,
                        alias,
                        selective: _,
                    } => {
                        // Load a Crush module via the module.load capability
                        // The capability is expected to push the module object onto the stack
                        instrs.push(self.create_instr(
                            "push_str",
                            serde_json::json!({"value": module_path}),
                            meta,
                        ));
                        instrs.push(self.create_instr(
                            "cap_call",
                            serde_json::json!({"name": "module.load", "argc": 1}),
                            meta,
                        ));
                        let store_name = alias.as_deref().unwrap_or(module_path.as_str());
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": store_name}),
                            meta,
                        ));
                    }
                    ImportStatement::Capability {
                        capability_path,
                        permissions: _,
                        alias,
                    } => {
                        // Register capability access — emit a cap_call to acquire a handle
                        instrs.push(self.create_instr(
                            "push_str",
                            serde_json::json!({"value": capability_path}),
                            meta,
                        ));
                        instrs.push(self.create_instr(
                            "cap_call",
                            serde_json::json!({"name": "cap.acquire", "argc": 1}),
                            meta,
                        ));
                        let store_name = alias.as_deref().unwrap_or(capability_path.as_str());
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": store_name}),
                            meta,
                        ));
                        // Track permission in manifest
                        self.all_permissions.insert(capability_path.clone());
                    }
                    ImportStatement::PolyglotModule {
                        language,
                        module_path,
                        alias,
                        selective: _,
                    } => {
                        // Load a polyglot module into the exec_lang session state
                        let load_code = format!("import {}", module_path);
                        instrs.push(self.create_instr(
                            "exec_lang",
                            serde_json::json!({
                                "lang": language,
                                "code": load_code,
                                "var_count": 0
                            }),
                            meta,
                        ));
                        let store_name = alias.as_deref().unwrap_or(module_path.as_str());
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": store_name}),
                            meta,
                        ));
                    }
                    ImportStatement::SecureEnv {
                        keys,
                        alias,
                        db_path: _,
                    } => {
                        // Load secrets via the secrets.read capability
                        self.all_permissions.insert("secrets.read".to_string());
                        if keys.is_empty() {
                            // Import all secrets as a module object
                            instrs.push(self.create_instr(
                                "cap_call",
                                serde_json::json!({"name": "secrets.load_all", "argc": 0}),
                                meta,
                            ));
                            let store_name = alias.as_deref().unwrap_or("secrets");
                            instrs.push(self.create_instr(
                                "store",
                                serde_json::json!({"name": store_name}),
                                meta,
                            ));
                        } else {
                            for key in keys {
                                instrs.push(self.create_instr(
                                    "push_str",
                                    serde_json::json!({"value": key}),
                                    meta,
                                ));
                                instrs.push(self.create_instr(
                                    "cap_call",
                                    serde_json::json!({"name": "secrets.read", "argc": 1}),
                                    meta,
                                ));
                                instrs.push(self.create_instr(
                                    "store",
                                    serde_json::json!({"name": key}),
                                    meta,
                                ));
                            }
                        }
                    }
                    ImportStatement::MCPImport {
                        server_url,
                        tools,
                        alias,
                    } => {
                        // Connect to MCP server and load tools
                        self.all_permissions.insert("mcp.client".to_string());
                        instrs.push(self.create_instr(
                            "push_str",
                            serde_json::json!({"value": server_url}),
                            meta,
                        ));
                        instrs.push(self.create_instr(
                            "cap_call",
                            serde_json::json!({"name": "mcp.connect", "argc": 1}),
                            meta,
                        ));
                        let store_name = alias.as_deref().unwrap_or("mcp");
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": store_name}),
                            meta,
                        ));
                        // Register each tool as a named variable
                        for tool in tools {
                            instrs.push(self.create_instr(
                                "load",
                                serde_json::json!({"name": store_name}),
                                meta,
                            ));
                            instrs.push(self.create_instr(
                                "push_str",
                                serde_json::json!({"value": tool}),
                                meta,
                            ));
                            instrs.push(self.create_instr(
                                "cap_call",
                                serde_json::json!({"name": "mcp.get_tool", "argc": 2}),
                                meta,
                            ));
                            instrs.push(self.create_instr(
                                "store",
                                serde_json::json!({"name": tool}),
                                meta,
                            ));
                        }
                    }
                    ImportStatement::External {
                        uri,
                        resource_type: _,
                        alias,
                    } => {
                        // Load external resource via the external.load capability
                        self.all_permissions.insert("external.load".to_string());
                        instrs.push(self.create_instr(
                            "push_str",
                            serde_json::json!({"value": uri}),
                            meta,
                        ));
                        instrs.push(self.create_instr(
                            "cap_call",
                            serde_json::json!({"name": "external.load", "argc": 1}),
                            meta,
                        ));
                        let store_name = alias.as_deref().unwrap_or("imported_resource");
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": store_name}),
                            meta,
                        ));
                    }
                }
            }
            Statement::DomMutate {
                target,
                mutation_type,
                value,
                value2,
                meta,
            } => {
                // Compile target expression
                self.compile_expr(target, instrs)?;

                // Compile value if present
                if let Some(val) = value {
                    self.compile_expr(val, instrs)?;
                }

                // Compile value2 if present
                if let Some(val2) = value2 {
                    self.compile_expr(val2, instrs)?;
                }

                // Emit dom_mutate instruction
                let mutation_str = match mutation_type {
                    DomMutationType::SetTextContent => "setTextContent",
                    DomMutationType::SetAttribute => "setAttribute",
                    DomMutationType::RemoveAttribute => "removeAttribute",
                    DomMutationType::SetStyle => "setStyle",
                    DomMutationType::SetInnerHtml => "setInnerHtml",
                    DomMutationType::AppendHtml => "appendHtml",
                    DomMutationType::Remove => "remove",
                    DomMutationType::AddClass => "addClass",
                    DomMutationType::RemoveClass => "removeClass",
                };

                instrs.push(self.create_instr(
                    "dom_mutate",
                    serde_json::json!({
                        "mutation": mutation_str,
                        "has_value": value.is_some(),
                        "has_value2": value2.is_some()
                    }),
                    meta,
                ));
            }
            Statement::DomEventListener {
                target,
                event,
                callback,
                meta,
            } => {
                self.compile_expr(target, instrs)?;
                self.compile_expr(callback, instrs)?;
                instrs.push(self.create_instr(
                    "dom_event_listener",
                    serde_json::json!({"event": event}),
                    meta,
                ));
            }
            Statement::AI(ai_stmt) => {
                // Compile AI-specific statements
                let ai_meta: HashMap<String, serde_json::Value> = HashMap::new();
                self.compile_ai_statement(ai_stmt, instrs, &ai_meta)?;
            }
        }
        Ok(())
    }

    fn compile_ai_statement(
        &mut self,
        stmt: &crush_cast::ai::AIStatement,
        instrs: &mut Vec<Instruction>,
        meta: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        use crush_cast::ai::*;

        match stmt {
            AIStatement::GoalDeclaration {
                goal_id,
                description,
                success_criteria,
                priority,
                deadline,
            } => {
                // Compile goal declaration as a call to AI runtime
                instrs.push(self.create_instr(
                    "ai_goal_decl",
                    serde_json::json!({
                        "goal_id": goal_id,
                        "description": description,
                        "success_criteria": success_criteria,
                        "priority": format!("{:?}", priority),
                        "deadline": deadline
                    }),
                    meta,
                ));
            }
            AIStatement::ProgressUpdate {
                goal_id,
                progress,
                status_message,
                metrics,
            } => {
                instrs.push(self.create_instr(
                    "ai_progress_update",
                    serde_json::json!({
                        "goal_id": goal_id,
                        "progress": progress,
                        "status_message": status_message,
                        "metrics": metrics
                    }),
                    meta,
                ));
            }
            AIStatement::KnowledgeSharing {
                knowledge_type,
                content,
                recipients,
                retention_policy,
            } => {
                instrs.push(self.create_instr(
                    "ai_knowledge_share",
                    serde_json::json!({
                        "knowledge_type": format!("{:?}", knowledge_type),
                        "content": content,
                        "recipients": recipients,
                        "retention_policy": format!("{:?}", retention_policy)
                    }),
                    meta,
                ));
            }
            AIStatement::CapabilityDiscovery {
                domain,
                requirements,
                discovery_strategy,
            } => {
                instrs.push(self.create_instr(
                    "ai_capability_discovery",
                    serde_json::json!({
                        "domain": domain,
                        "requirements": requirements,
                        "discovery_strategy": format!("{:?}", discovery_strategy)
                    }),
                    meta,
                ));
            }
            AIStatement::AdaptationRequest {
                adaptation_type,
                reason,
                parameters,
            } => {
                instrs.push(self.create_instr(
                    "ai_adaptation_request",
                    serde_json::json!({
                        "adaptation_type": format!("{:?}", adaptation_type),
                        "reason": reason,
                        "parameters": parameters
                    }),
                    meta,
                ));
            }
        }
        Ok(())
    }

    fn compile_expr(&mut self, expr: &Expression, instrs: &mut Vec<Instruction>) -> Result<()> {
        self.compile_expr_with_name_hint(expr, instrs, None)
    }

    fn compile_expr_with_name_hint(
        &mut self,
        expr: &Expression,
        instrs: &mut Vec<Instruction>,
        name_hint: Option<&str>,
    ) -> Result<()> {
        match expr {
            Expression::IntLiteral { value, meta } => {
                instrs.push(self.create_instr(
                    "push_int",
                    serde_json::json!({"value": value}),
                    meta,
                ));
            }
            Expression::FloatLiteral { value, meta } => {
                instrs.push(self.create_instr(
                    "push_float",
                    serde_json::json!({"value": value}),
                    meta,
                ));
            }
            Expression::StringLiteral { value, meta } => {
                instrs.push(self.create_instr(
                    "push_str",
                    serde_json::json!({"value": value}),
                    meta,
                ));
            }
            Expression::BoolLiteral { value, meta } => {
                instrs.push(self.create_instr(
                    "push_bool",
                    serde_json::json!({"value": value}),
                    meta,
                ));
            }
            Expression::NullLiteral { meta } => {
                instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
            }
            Expression::Var { name, meta } => {
                instrs.push(self.create_instr("load", serde_json::json!({"name": name}), meta));
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                meta,
            } => {
                self.compile_expr(left, instrs)?;
                self.compile_expr(right, instrs)?;
                let op_code = match operator.as_str() {
                    "+" => "add",
                    "-" => "sub",
                    "*" => "mul",
                    "/" => "div",
                    "%" => "mod",
                    "==" => "eq",
                    "!=" => "ne",
                    "<" => "lt",
                    ">" => "gt",
                    "<=" => "le",
                    ">=" => "ge",
                    "and" => "and",
                    "or" => "or",
                    "&&" => "and",
                    "||" => "or",
                    _ => bail!("Unsupported op: {}", operator),
                };
                instrs.push(self.create_instr(op_code, serde_json::json!({}), meta));
            }
            Expression::UnaryOp {
                operator,
                operand,
                meta,
            } => {
                self.compile_expr(operand, instrs)?;
                let op_code = match operator.as_str() {
                    "-" => "neg",
                    "not" => "not",
                    _ => bail!("Unsupported op: {}", operator),
                };
                instrs.push(self.create_instr(op_code, serde_json::json!({}), meta));
            }
            Expression::Call {
                function,
                args,
                meta,
            } => {
                if function == "len" {
                    if args.len() != 1 {
                        bail!("len() expects exactly 1 argument");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("len", serde_json::json!({}), meta));
                } else if function == "print" {
                    self.all_permissions.insert("io.print".to_string());
                    for arg in args {
                        self.compile_expr(arg, instrs)?;
                    }
                    instrs.push(self.create_instr(
                        "cap_call",
                        serde_json::json!({"name": "io.print", "argc": args.len()}),
                        meta,
                    ));
                } else if function == "str.contains" {
                    if args.len() != 2 {
                        bail!("str.contains() expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("str_contains", serde_json::json!({}), meta));
                } else if function == "str.split" {
                    if args.len() != 2 {
                        bail!("str.split() expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("str_split", serde_json::json!({}), meta));
                } else if function == "str.replace" {
                    if args.len() != 3 {
                        bail!("str.replace() expects exactly 3 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    self.compile_expr(&args[2], instrs)?;
                    instrs.push(self.create_instr("str_replace", serde_json::json!({}), meta));
                } else if function == "str.join" {
                    if args.len() != 2 {
                        bail!("str.join() expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("str_join", serde_json::json!({}), meta));
                } else if function == "array.push" {
                    if args.len() != 2 {
                        bail!("array.push() expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("array_push", serde_json::json!({}), meta));
                } else if function == "array.pop" {
                    if args.len() != 1 {
                        bail!("array.pop() expects exactly 1 argument");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("array_pop", serde_json::json!({}), meta));
                } else {
                    for arg in args {
                        self.compile_expr(arg, instrs)?;
                    }
                    instrs.push(self.create_instr(
                        "call",
                        serde_json::json!({"function": function, "argc": args.len()}),
                        meta,
                    ));
                }
            }
            Expression::CapabilityCall { name, args, meta } => {
                if name == "len" {
                    if args.len() != 1 {
                        bail!("len() expects exactly 1 argument");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("len", serde_json::json!({}), meta));
                } else if self.local_functions.contains(name) {
                    for arg in args {
                        self.compile_expr(arg, instrs)?;
                    }
                    instrs.push(self.create_instr(
                        "call",
                        serde_json::json!({"function": name, "argc": args.len()}),
                        meta,
                    ));
                } else {
                    self.all_permissions.insert(name.clone());
                    for arg in args {
                        self.compile_expr(arg, instrs)?;
                    }
                    instrs.push(self.create_instr(
                        "cap_call",
                        serde_json::json!({"name": name, "argc": args.len()}),
                        meta,
                    ));
                }
            }
            Expression::Pipeline { segments, meta } => {
                if segments.is_empty() {
                    instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
                } else {
                    // Compile first segment normally
                    self.compile_expr(&segments[0], instrs)?;

                    // Subsequent segments take previous result as first arg
                    for segment in &segments[1..] {
                        match segment {
                            Expression::Call {
                                function,
                                args,
                                meta,
                            } => {
                                for arg in args {
                                    self.compile_expr(arg, instrs)?;
                                }
                                instrs.push(self.create_instr("call", serde_json::json!({"function": function, "argc": args.len() + 1}), meta));
                            }
                            Expression::CapabilityCall { name, args, meta } => {
                                if self.local_functions.contains(name) {
                                    for arg in args {
                                        self.compile_expr(arg, instrs)?;
                                    }
                                    instrs.push(self.create_instr(
                                        "call",
                                        serde_json::json!({"function": name, "argc": args.len() + 1}),
                                        meta,
                                    ));
                                } else {
                                    self.all_permissions.insert(name.clone());
                                    for arg in args {
                                        self.compile_expr(arg, instrs)?;
                                    }
                                    instrs.push(self.create_instr(
                                        "cap_call",
                                        serde_json::json!({"name": name, "argc": args.len() + 1}),
                                        meta,
                                    ));
                                }
                            }
                            Expression::Var { name, meta } => {
                                instrs.push(self.create_instr(
                                    "call",
                                    serde_json::json!({"function": name, "argc": 1}),
                                    meta,
                                ));
                            }
                            _ => {
                                // For other expressions, we might just evaluate them and pop the previous result?
                                // Or maybe it's use-less. Let's just evaluate it and it will likely error at runtime if it expects args.
                                // Actually, let's treat it as a call if possible, or just evaluate.
                                self.compile_expr(segment, instrs)?;
                            }
                        }
                    }
                }
            }
            Expression::Spawn {
                function,
                args,
                meta,
            } => {
                if !args.is_empty() {
                    bail!(
                        "spawn does not currently support arguments. Function must take 0 arguments."
                    );
                }
                instrs.push(self.create_instr(
                    "push_str",
                    serde_json::json!({"value": function}),
                    meta,
                ));
                instrs.push(self.create_instr("spawn", serde_json::json!({}), meta));
            }
            Expression::ArrayLiteral { elements, meta } => {
                instrs.push(self.create_instr(
                    "new_array",
                    serde_json::json!({"size": elements.len()}),
                    meta,
                ));

                for element in elements {
                    instrs.push(self.create_instr("dup", serde_json::json!({}), meta));
                    self.compile_expr(element, instrs)?;
                    instrs.push(self.create_instr("array_push", serde_json::json!({}), meta));
                }
            }
            Expression::Yield { meta } => {
                instrs.push(self.create_instr("yield", serde_json::json!({}), meta));
            }
            Expression::NewStruct { name, meta } => {
                instrs.push(self.create_instr(
                    "new_struct",
                    serde_json::json!({"name": name}),
                    meta,
                ));
            }
            Expression::GetField {
                target,
                field,
                meta,
            } => {
                self.compile_expr(target, instrs)?;
                instrs.push(self.create_instr(
                    "get_field",
                    serde_json::json!({"name": field}),
                    meta,
                ));
            }
            Expression::Range { start, end, meta } => {
                // Compile range as array creation: [start..end]
                self.compile_expr(start, instrs)?;
                self.compile_expr(end, instrs)?;
                instrs.push(self.create_instr("make_range", serde_json::json!({}), meta));
            }
            Expression::Await { expression, meta } => {
                // For MVP: just compile the expression
                // In future: this will poll futures
                self.compile_expr(expression, instrs)?;
                instrs.push(self.create_instr("await", serde_json::json!({}), meta));
            }
            Expression::Index {
                target,
                index,
                meta,
            } => {
                self.compile_expr(target, instrs)?;
                self.compile_expr(index, instrs)?;
                instrs.push(self.create_instr("index", serde_json::json!({}), meta));
            }
            Expression::ObjectLiteral { properties, meta } => {
                // Create a new object/map
                instrs.push(self.create_instr("new_obj", serde_json::json!({}), meta));
                // Set each property
                for (key, value) in properties {
                    instrs.push(self.create_instr("dup", serde_json::json!({}), meta)); // Dup object ref
                    self.compile_expr(value, instrs)?;
                    instrs.push(self.create_instr(
                        "set_field",
                        serde_json::json!({"name": key}),
                        meta,
                    ));
                }
            }
            Expression::DomQuery {
                query_type,
                selector,
                meta,
            } => {
                self.compile_expr(selector, instrs)?;
                let query_str = match query_type {
                    DomQueryType::QuerySelector => "querySelector",
                    DomQueryType::QuerySelectorAll => "querySelectorAll",
                    DomQueryType::GetElementById => "getElementById",
                    DomQueryType::GetElementsByClassName => "getElementsByClassName",
                    DomQueryType::GetElementsByTagName => "getElementsByTagName",
                };
                instrs.push(self.create_instr(
                    "dom_query",
                    serde_json::json!({"query_type": query_str}),
                    meta,
                ));
            }
            Expression::Lambda { params, body, meta } => {
                let lambda_name = if let Some(hint) = name_hint {
                    hint.to_string()
                } else {
                    format!("__lambda_{}", self.temp_counter)
                };
                self.temp_counter += 1;
                self.local_functions.insert(lambda_name.clone());

                let mut func_instrs = Vec::new();
                for (param_name, _) in params {
                    func_instrs.push(self.create_instr(
                        "store",
                        serde_json::json!({"name": param_name}),
                        meta,
                    ));
                }
                for inner_stmt in body {
                    self.compile_stmt(inner_stmt, &mut func_instrs)?;
                }
                self.ensure_return(&mut func_instrs, Some(meta));

                self.lambdas.insert(
                    lambda_name.clone(),
                    CasmFunction {
                        params: params.iter().map(|(n, _)| n.clone()).collect(),
                        locals: vec![],
                        body: func_instrs,
                    },
                );

                instrs.push(self.create_instr(
                    "push_str",
                    serde_json::json!({"value": lambda_name}),
                    meta,
                ));
            }
            Expression::Match { expression, arms, meta } => {
                self.compile_expr(expression, instrs)?;
                let temp_var = format!("__match_val_{}", self.temp_counter);
                self.temp_counter += 1;
                instrs.push(self.create_instr(
                    "store",
                    serde_json::json!({"name": temp_var}),
                    meta,
                ));

                let mut end_jumps: Vec<usize> = Vec::new();
                let mut prev_fail_jumps: Vec<usize> = Vec::new();

                for (arm_idx, arm) in arms.iter().enumerate() {
                    if arm_idx > 0 {
                        let arm_start = instrs.len();
                        for jmp_idx in prev_fail_jumps {
                            instrs[jmp_idx].args = serde_json::json!({"target": arm_start});
                        }
                    }

                    let mut current_fail_jumps = Vec::new();
                    self.compile_pattern(&arm.pattern, &temp_var, instrs, &mut current_fail_jumps)?;

                    if arm.body.is_empty() {
                        instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
                    } else {
                        for stmt in &arm.body[..arm.body.len() - 1] {
                            self.compile_stmt(stmt, instrs)?;
                        }
                        let last_stmt = &arm.body[arm.body.len() - 1];
                        if let Statement::ExprStmt { expr: last_expr, .. } = last_stmt {
                            self.compile_expr(last_expr, instrs)?;
                        } else {
                            self.compile_stmt(last_stmt, instrs)?;
                            instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
                        }
                    }

                    end_jumps.push(instrs.len());
                    instrs.push(self.create_instr(
                        "jmp",
                        serde_json::json!({"target": 0}),
                        meta,
                    ));

                    prev_fail_jumps = current_fail_jumps;
                }

                let match_end = instrs.len();
                for jmp_idx in prev_fail_jumps {
                    instrs[jmp_idx].args = serde_json::json!({"target": match_end});
                }

                instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));

                let match_end_final = instrs.len();
                for jmp_idx in end_jumps {
                    instrs[jmp_idx].args = serde_json::json!({"target": match_end_final});
                }
            }
            Expression::AI(ai_expr) => {
                // Compile AI-specific expressions
                let ai_meta: HashMap<String, serde_json::Value> = HashMap::new();
                self.compile_ai_expression(ai_expr, instrs, &ai_meta)?;
            }
        }
        Ok(())
    }

    fn compile_ai_expression(
        &mut self,
        expr: &crush_cast::ai::AIExpression,
        instrs: &mut Vec<Instruction>,
        meta: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        use crush_cast::ai::*;

        match expr {
            AIExpression::Query {
                query,
                result_type,
                context,
            } => {
                instrs.push(self.create_instr(
                    "ai_query",
                    serde_json::json!({
                        "query": query,
                        "result_type": result_type,
                        "context": context
                    }),
                    meta,
                ));
            }
            AIExpression::ToolChain {
                tools,
                strategy,
                error_handling,
            } => {
                // Compile tool chain - serialize tools and strategy
                let tools_json: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "tool_name": t.tool_name,
                            "parameters": t.parameters,
                            "result_binding": t.result_binding,
                            "condition": t.condition
                        })
                    })
                    .collect();

                let strategy_json = match strategy {
                    ExecutionStrategy::Sequential => serde_json::json!({"type": "sequential"}),
                    ExecutionStrategy::Parallel => serde_json::json!({"type": "parallel"}),
                    ExecutionStrategy::Conditional { conditions } => {
                        serde_json::json!({
                            "type": "conditional",
                            "conditions": conditions
                        })
                    }
                    ExecutionStrategy::Retry {
                        max_attempts,
                        backoff_strategy,
                    } => {
                        serde_json::json!({
                            "type": "retry",
                            "max_attempts": max_attempts,
                            "backoff": format!("{:?}", backoff_strategy)
                        })
                    }
                };

                let error_handling_json = match error_handling {
                    ErrorHandling::FailFast => serde_json::json!({"type": "fail_fast"}),
                    ErrorHandling::ContinueOnError => {
                        serde_json::json!({"type": "continue_on_error"})
                    }
                    ErrorHandling::Retry {
                        max_retries,
                        retry_condition,
                    } => {
                        serde_json::json!({
                            "type": "retry",
                            "max_retries": max_retries,
                            "retry_condition": retry_condition
                        })
                    }
                    ErrorHandling::Fallback { fallback_tools } => {
                        serde_json::json!({
                            "type": "fallback",
                            "fallback_count": fallback_tools.len()
                        })
                    }
                };

                instrs.push(self.create_instr(
                    "ai_tool_chain",
                    serde_json::json!({
                        "tools": tools_json,
                        "strategy": strategy_json,
                        "error_handling": error_handling_json
                    }),
                    meta,
                ));
            }
            AIExpression::AgentDelegation {
                task,
                agents,
                delegation_strategy,
                expected_format,
            } => {
                let strategy_json = match delegation_strategy {
                    DelegationStrategy::FirstAvailable => {
                        serde_json::json!({"type": "first_available"})
                    }
                    DelegationStrategy::CapabilityMatch => {
                        serde_json::json!({"type": "capability_match"})
                    }
                    DelegationStrategy::ParallelSplit => {
                        serde_json::json!({"type": "parallel_split"})
                    }
                    DelegationStrategy::Hierarchical => serde_json::json!({"type": "hierarchical"}),
                    DelegationStrategy::Consensus { threshold } => {
                        serde_json::json!({"type": "consensus", "threshold": threshold})
                    }
                    DelegationStrategy::Broadcast => {
                        serde_json::json!({"type": "broadcast"})
                    }
                    DelegationStrategy::Best => {
                        serde_json::json!({"type": "best"})
                    }
                    DelegationStrategy::RoundRobin => {
                        serde_json::json!({"type": "round_robin"})
                    }
                };

                instrs.push(self.create_instr(
                    "ai_agent_delegation",
                    serde_json::json!({
                        "task": task,
                        "agents": agents,
                        "strategy": strategy_json,
                        "expected_format": expected_format
                    }),
                    meta,
                ));
            }
            AIExpression::LearningLoop {
                learning_target,
                strategy,
                adaptations,
            } => {
                let target_json = match learning_target {
                    LearningTarget::UserBehavior => serde_json::json!({"type": "user_behavior"}),
                    LearningTarget::ExecutionPatterns => {
                        serde_json::json!({"type": "execution_patterns"})
                    }
                    LearningTarget::ErrorPatterns => serde_json::json!({"type": "error_patterns"}),
                    LearningTarget::PerformanceMetrics => {
                        serde_json::json!({"type": "performance_metrics"})
                    }
                    LearningTarget::ToolUsage => serde_json::json!({"type": "tool_usage"}),
                };

                let strategy_json = match strategy {
                    LearningStrategy::PatternRecognition => {
                        serde_json::json!({"type": "pattern_recognition"})
                    }
                    LearningStrategy::StatisticalAnalysis => {
                        serde_json::json!({"type": "statistical_analysis"})
                    }
                    LearningStrategy::MachineLearning => {
                        serde_json::json!({"type": "machine_learning"})
                    }
                    LearningStrategy::RuleBased => serde_json::json!({"type": "rule_based"}),
                };

                let adaptations_json: Vec<&str> = adaptations
                    .iter()
                    .map(|a| match a {
                        AdaptationAction::OptimizeToolChain => "optimize_tool_chain",
                        AdaptationAction::ImproveErrorHandling => "improve_error_handling",
                        AdaptationAction::UpdateAgentSelection => "update_agent_selection",
                        AdaptationAction::ModifyExecutionStrategy => "modify_execution_strategy",
                        AdaptationAction::LearnNewPatterns => "learn_new_patterns",
                    })
                    .collect();

                instrs.push(self.create_instr(
                    "ai_learning_loop",
                    serde_json::json!({
                        "learning_target": target_json,
                        "strategy": strategy_json,
                        "adaptations": adaptations_json
                    }),
                    meta,
                ));
            }
            AIExpression::ContextAware {
                expression,
                requires_context,
                provides_context,
            } => {
                // First compile the wrapped expression
                self.compile_expr(expression, instrs)?;

                instrs.push(self.create_instr(
                    "ai_context_aware",
                    serde_json::json!({
                        "requires_context": requires_context,
                        "provides_context": provides_context
                    }),
                    meta,
                ));
            }
        }
        Ok(())
    }

    fn create_instr(
        &self,
        op: &str,
        args: serde_json::Value,
        meta: &HashMap<String, serde_json::Value>,
    ) -> Instruction {
        let meta_json = serde_json::to_value(meta).unwrap();
        let lang = meta
            .get("lang")
            .and_then(|l| l.as_str())
            .map(|s| s.to_string());
        Instruction {
            op: op.to_string(),
            args,
            lang,
            meta: Some(meta_json),
        }
    }
}
