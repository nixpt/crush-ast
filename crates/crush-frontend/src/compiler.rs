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
    /// Variable names declared in the current function scope (from VarDecl / Assign).
    declared_vars: HashSet<String>,
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
            declared_vars: HashSet::new(),
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
            self.declared_vars.clear();
            for stmt in &mut func.body {
                match stmt {
                    Statement::VarDecl { name, .. } => {
                        declared.push(name.clone());
                        self.declared_vars.insert(name.clone());
                    }
                    Statement::Assign { target, .. } => {
                        declared.push(target.clone());
                        self.declared_vars.insert(target.clone());
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

                    let hints = extract_type_hints(params, body);
                    casm_program.functions.insert(
                        name.clone(),
                        CasmFunction {
                            params: params.iter().map(|(n, _)| n.clone()).collect(),
                            locals: vec![],
                            type_hints: if hints.is_empty() { None } else { Some(hints) },
                            body: func_instrs,
                        },
                    );
                }
            }

            // Second pass: Compile main function instructions
            self.declared_vars.clear();
            // Add function params to declared vars (before store instructions)
            for (param_name, _) in &func.params {
                self.declared_vars.insert(param_name.clone());
            }
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
            if func.is_async {
                let inner_name = format!("{}_async_inner", name);
                let mut inner_instrs = Vec::new();

                for (param_name, _) in &func.params {
                    inner_instrs.push(self.create_instr(
                        "store",
                        serde_json::json!({"name": param_name}),
                        &func.meta,
                    ));
                }

                for stmt in &func.body {
                    if !matches!(stmt, Statement::FunctionDef { .. }) {
                        self.compile_stmt(stmt, &mut inner_instrs)?;
                    }
                }

                self.ensure_return(&mut inner_instrs, Some(&func.meta));
                self.record_debug_info_for_function(&inner_name, &inner_instrs, &mut debug_info, &mut source_files);

                let hints = extract_type_hints(&func.params, &func.body);
                casm_program.functions.insert(
                    inner_name.clone(),
                    CasmFunction {
                        params: func.params.iter().map(|(n, _)| n.clone()).collect(),
                        locals: vec![],
                        type_hints: if hints.is_empty() { None } else { Some(hints) },
                        body: inner_instrs,
                    },
                );

                for (param_name, _) in &func.params {
                    instrs.push(self.create_instr(
                        "load",
                        serde_json::json!({"name": param_name}),
                        &func.meta,
                    ));
                }

                instrs.push(self.create_instr(
                    "push_str",
                    serde_json::json!({"value": inner_name}),
                    &func.meta,
                ));

                instrs.push(self.create_instr(
                    "spawn",
                    serde_json::json!({"argc": func.params.len()}),
                    &func.meta,
                ));

                instrs.push(self.create_instr("ret", serde_json::json!({}), &func.meta));

                self.record_debug_info_for_function(&name, &instrs, &mut debug_info, &mut source_files);

                casm_program.functions.insert(
                    name.clone(),
                    CasmFunction {
                        params: func.params.iter().map(|(n, _)| n.clone()).collect(),
                        locals: vec![],
                        type_hints: None, // The outer async wrapper doesn't need type hints
                        body: instrs,
                    },
                );
            } else {
                for stmt in &func.body {
                    if !matches!(stmt, Statement::FunctionDef { .. }) {
                        self.compile_stmt(stmt, &mut instrs)?;
                    }
                }

                self.ensure_return(&mut instrs, Some(&func.meta));
                self.record_debug_info_for_function(&name, &instrs, &mut debug_info, &mut source_files);

                let hints = extract_type_hints(&func.params, &func.body);
                casm_program.functions.insert(
                    name.clone(),
                    CasmFunction {
                        params: func.params.iter().map(|(n, _)| n.clone()).collect(),
                        locals: vec![],
                        type_hints: if hints.is_empty() { None } else { Some(hints) },
                        body: instrs,
                    },
                );
            }
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
        // `for i in a..b` — a numeric range.
        //
        // Lowered directly rather than through the array path below: the loop variable IS the
        // counter, so no array/index temporaries are needed. Critically, a range must NEVER be
        // materialised as an array — `for i in 0..1_000_000` would allocate a million elements
        // to iterate integers.
        //
        //     i = start
        //     while __end > i { body; i = i + 1 }
        if let Statement::For { variable, iterable, body, meta } = stmt {
            if let Expression::Range { start, end, .. } = iterable.as_ref() {
                let end_var = format!("__end_{}", self.temp_counter);
                self.temp_counter += 1;

                self.compile_expr(start, instrs)?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": variable}), meta));
                self.compile_expr(end, instrs)?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": &end_var}), meta));

                self.loop_stack.push(LoopInfo { continue_target: 0, break_indices: Vec::new() });

                let loop_start = instrs.len();
                instrs.push(self.create_instr("load", serde_json::json!({"name": &end_var}), meta));
                instrs.push(self.create_instr("load", serde_json::json!({"name": variable}), meta));
                instrs.push(self.create_instr("gt", serde_json::json!({}), meta));
                let jmp_if_not_idx = instrs.len();
                instrs.push(self.create_instr("jmp_if_not", serde_json::json!({"target": 0}), meta));

                for st in body {
                    self.compile_stmt(st, instrs)?;
                }

                let continue_idx = instrs.len();
                self.loop_stack.last_mut().unwrap().continue_target = continue_idx;

                instrs.push(self.create_instr("load", serde_json::json!({"name": variable}), meta));
                instrs.push(self.create_instr("push_int", serde_json::json!({"value": 1}), meta));
                instrs.push(self.create_instr("add", serde_json::json!({}), meta));
                instrs.push(self.create_instr("store", serde_json::json!({"name": variable}), meta));

                instrs.push(self.create_instr("jmp", serde_json::json!({"target": loop_start}), meta));

                let loop_end = instrs.len();
                instrs[jmp_if_not_idx].args = serde_json::json!({"target": loop_end});
                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_indices {
                    instrs[idx].args = serde_json::json!({"target": loop_end});
                }
                return Ok(());
            }
        }

        match stmt {
            Statement::VarDecl {
                name, value, meta, ..
            } => {
                self.declared_vars.insert(name.clone());
                self.compile_expr_with_name_hint(value, instrs, Some(name))?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": name}), meta));
            }
            Statement::Assign {
                target, value, meta
            } => {
                self.declared_vars.insert(target.clone());
                self.compile_expr_with_name_hint(value, instrs, Some(target))?;
                instrs.push(self.create_instr("store", serde_json::json!({"name": target}), meta));
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
                // `yield` is modelled as an Expression in the CAST but produces NO value —
                // its instruction pushes nothing. Popping after it underflows the stack.
                // Every other expression leaves exactly one value behind.
                if !matches!(expr, Expression::Yield { .. }) {
                    instrs.push(self.create_instr("pop", serde_json::json!({}), meta));
                }
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

                // exec_lang always pushes a return value; it must always be
                // consumed — either stored into the block's designated
                // output variable (`meta["polyglot_output"]`, set by a
                // language-specific free-variable analysis pass before
                // compilation — see crush-lang-sdk::compile for Python) or
                // explicitly popped, never left to leak on the stack.
                match meta.get("polyglot_output").and_then(|v| v.as_str()) {
                    Some(output_var) => {
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": output_var}),
                            meta,
                        ));
                    }
                    None => {
                        instrs.push(self.create_instr("pop", serde_json::json!({}), meta));
                    }
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
            AIStatement::SemanticSwitch {
                target,
                cases,
                fallback,
            } => {
                self.compile_expr(target, instrs)?;
                
                let switch_idx = instrs.len();
                instrs.push(self.create_instr(
                    "ai_semantic_switch",
                    serde_json::json!({}), // We'll patch this later
                    meta,
                ));

                let mut switch_cases = Vec::new();
                let mut jump_to_end_indices = Vec::new();

                for (concept, block) in cases {
                    let start_idx = instrs.len();
                    for s in block {
                        self.compile_stmt(s, instrs)?;
                    }
                    switch_cases.push(serde_json::json!({
                        "concept": concept,
                        "target_pc": start_idx
                    }));
                    jump_to_end_indices.push(instrs.len());
                    instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), meta));
                }
                
                let fallback_target = if let Some(fb) = fallback {
                    let start_idx = instrs.len();
                    for s in fb {
                        self.compile_stmt(s, instrs)?;
                    }
                    Some(start_idx)
                } else {
                    None
                };

                let end_label = instrs.len();
                
                // Patch the switch instruction
                instrs[switch_idx].args = serde_json::json!({
                    "cases": switch_cases,
                    "fallback_target": fallback_target.unwrap_or(end_label)
                });

                // Patch all the jumps
                for idx in jump_to_end_indices {
                    instrs[idx].args = serde_json::json!({"target": end_label});
                }
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
                    "//" => "div",   // floor division: same as div for positive ints
                    "===" => "eq",    // JS strict equality
                    "!==" => "ne",    // JS strict inequality
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
                    "!" => "not",    // JS logical not
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
                    if args.len() != 2 { bail!("str.contains() expects exactly 2 arguments"); }
                    self.compile_expr(&args[0], instrs)?; self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("str_contains", serde_json::json!({}), meta));
                } else if function == "str.starts_with" {
                    if args.len() != 2 { bail!("str.starts_with() expects exactly 2 arguments"); }
                    self.compile_expr(&args[0], instrs)?; self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("str_starts_with", serde_json::json!({}), meta));
                } else if function == "str.ends_with" {
                    if args.len() != 2 { bail!("str.ends_with() expects exactly 2 arguments"); }
                    self.compile_expr(&args[0], instrs)?; self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("str_ends_with", serde_json::json!({}), meta));
                } else if function == "str.to_upper" {
                    if args.len() != 1 { bail!("str.to_upper() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("str_to_upper", serde_json::json!({}), meta));
                } else if function == "str.to_lower" {
                    if args.len() != 1 { bail!("str.to_lower() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("str_to_lower", serde_json::json!({}), meta));
                } else if function == "str.trim" {
                    if args.len() != 1 { bail!("str.trim() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("str_trim", serde_json::json!({}), meta));
                } else if function == "math.pow" {
                    if args.len() != 2 { bail!("math.pow() expects exactly 2 arguments"); }
                    self.compile_expr(&args[0], instrs)?; self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("math_pow", serde_json::json!({}), meta));
                } else if function == "math.sqrt" {
                    if args.len() != 1 { bail!("math.sqrt() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("math_sqrt", serde_json::json!({}), meta));
                } else if function == "math.abs" {
                    if args.len() != 1 { bail!("math.abs() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("math_abs", serde_json::json!({}), meta));
                } else if function == "math.round" {
                    if args.len() != 1 { bail!("math.round() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("math_round", serde_json::json!({}), meta));
                } else if function == "math.floor" {
                    if args.len() != 1 { bail!("math.floor() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("math_floor", serde_json::json!({}), meta));
                } else if function == "math.ceil" {
                    if args.len() != 1 { bail!("math.ceil() expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("math_ceil", serde_json::json!({}), meta));
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
                } else if function == "make_range" {
                    // make_range handles 0..2 args at capability runtime
                    let argc = args.len();
                    for arg in args { self.compile_expr(arg, instrs)?; }
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "make_range", "argc": argc}), meta));
                } else if function == "arr_set" {
                    if args.len() != 3 {
                        bail!("arr_set() expects exactly 3 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    self.compile_expr(&args[2], instrs)?;
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "arr_set", "argc": 3}), meta));
                } else if function == "arr_get" {
                    if args.len() != 2 {
                        bail!("arr_get() expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "arr_get", "argc": 2}), meta));
                } else if function == "array.push" {
                    if args.len() != 2 {
                        bail!("array.push() expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("array_push", serde_json::json!({}), meta));
                } else if function == "range" || function == "make_range" {
                    let argc = args.len();
                    for arg in args { self.compile_expr(arg, instrs)?; }
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "make_range", "argc": argc}), meta));
                } else if function == "array.pop" {
                    if args.len() != 1 {
                        bail!("array.pop() expects exactly 1 argument");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("array_pop", serde_json::json!({}), meta));
                } else if function == "__crush_setindex__" {
                    // Intrinsic: indexed assignment lowered from walker (e.g., `arr[i] = val`)
                    // args[0] = target, args[1] = index, args[2] = value
                    if args.len() != 3 {
                        bail!("__crush_setindex__ expects exactly 3 arguments");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    self.compile_expr(&args[2], instrs)?;
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "arr_set", "argc": 3}), &meta));
                } else if function == "__crush_assign__" {
                    // Intrinsic: assignment lowered from walker (e.g., `x = 42` in JS)
                    // args[0] = target Var, args[1] = value expression
                    if args.len() != 2 {
                        bail!("__crush_assign__ expects exactly 2 arguments");
                    }
                    self.compile_expr(&args[1], instrs)?;
                    if let Expression::Var { name, meta: _ } = &args[0] {
                        instrs.push(self.create_instr(
                            "store",
                            serde_json::json!({"name": name}),
                            &meta,
                        ));
                        // store pops the value; push null so the enclosing ExprStmt pop doesn't underflow
                        instrs.push(self.create_instr(
                            "push_null",
                            serde_json::json!({}),
                            &meta,
                        ));
                    } else {
                        bail!("__crush_assign__ target must be a variable");
                    }
                } else if function == "__crush_not__" {
                    if args.len() != 1 { bail!("__crush_not__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("not", serde_json::json!({}), &meta));
                } else if function == "__crush_bit_not__" {
                    if args.len() != 1 { bail!("__crush_bit_not__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("bit_not", serde_json::json!({}), &meta));
                } else if function == "__crush_neg__" {
                    if args.len() != 1 { bail!("__crush_neg__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("neg", serde_json::json!({}), &meta));
                } else if function == "__crush_pos__" {
                    if args.len() != 1 { bail!("__crush_pos__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    // unary plus is identity — result is already on stack
                } else if function == "__crush_subscript__" {
                    if args.len() != 2 { bail!("__crush_subscript__ expects exactly 2 arguments"); }
                    self.compile_expr(&args[0], instrs)?;
                    self.compile_expr(&args[1], instrs)?;
                    instrs.push(self.create_instr("arr_get", serde_json::json!({}), &meta));
                } else if function == "__crush_ternary__" {
                    if args.len() != 3 { bail!("__crush_ternary__ expects exactly 3 arguments"); }
                    self.compile_expr(&args[0], instrs)?;
                    let jmp_if_not_idx = instrs.len();
                    instrs.push(self.create_instr("jmp_if_not", serde_json::json!({"target": 0}), &meta));
                    self.compile_expr(&args[1], instrs)?;
                    let jmp_end_idx = instrs.len();
                    instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), &meta));
                    let else_start = instrs.len();
                    instrs[jmp_if_not_idx].args = serde_json::json!({"target": else_start});
                    self.compile_expr(&args[2], instrs)?;
                    let end_label = instrs.len();
                    instrs[jmp_end_idx].args = serde_json::json!({"target": end_label});
                } else if function == "__crush_pre_inc__" || function == "__crush_post_inc__"
                    || function == "__crush_pre_dec__" || function == "__crush_post_dec__"
                {
                    if args.len() != 1 { bail!("{function} expects exactly 1 argument"); }
                    let var_name = if let Expression::Var { name, .. } = &args[0] {
                        name.clone()
                    } else {
                        bail!("{function} target must be a variable");
                    };
                    let is_inc = function.contains("inc");
                    let is_pre = function.contains("pre");
                    self.compile_expr(&args[0], instrs)?;  // load var
                    if !is_pre {
                        instrs.push(self.create_instr("dup", serde_json::json!({}), &meta));
                    }
                    instrs.push(self.create_instr("push_int", serde_json::json!({"value": 1}), &meta));
                    if is_inc {
                        instrs.push(self.create_instr("add", serde_json::json!({}), &meta));
                    } else {
                        instrs.push(self.create_instr("sub", serde_json::json!({}), &meta));
                    }
                    if is_pre {
                        instrs.push(self.create_instr("dup", serde_json::json!({}), &meta));
                    }
                    instrs.push(self.create_instr("store", serde_json::json!({"name": var_name}), &meta));
                } else if function == "__crush_deref__" {
                    if args.len() != 1 { bail!("__crush_deref__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "__crush_deref__", "argc": 1}), &meta));
                } else if function == "__crush_addr_of__" {
                    if args.len() != 1 { bail!("__crush_addr_of__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "__crush_addr_of__", "argc": 1}), &meta));
                } else if function == "__crush_unary__" {
                    if args.len() != 1 { bail!("__crush_unary__ expects exactly 1 argument"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "__crush_unary__", "argc": 1}), &meta));
                } else {
                    // Check for method-call syntax: obj.method(args)
                    if let Some(dot_pos) = function.find('.') {
                        let obj_name = &function[..dot_pos];
                        let method = &function[dot_pos + 1..];
                        instrs.push(self.create_instr("load", serde_json::json!({"name": obj_name}), &meta));
                        // Push args in reverse so callee's store pops them correctly
                        for arg in args.iter().rev() {
                            self.compile_expr(arg, instrs)?;
                        }
                        instrs.push(self.create_instr("cap_call", serde_json::json!({"name": method, "argc": args.len() + 1}), &meta));
                    } else {
                        // Push args in REVERSE order so the callee's `store param` instructions
                        // pop them in the correct order (stack is LIFO: last pushed = first popped).
                        for arg in args.iter().rev() {
                            self.compile_expr(arg, instrs)?;
                        }
                        instrs.push(self.create_instr("call", serde_json::json!({"function": function, "argc": args.len()}), &meta));
                    }
                }
            }
            Expression::VectorMath { operator, args, meta } => {
                for arg in args {
                    self.compile_expr(arg, instrs)?;
                }
                instrs.push(self.create_instr(operator, serde_json::json!({}), meta));
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
                    // Check for method-call syntax: obj.method(args)
                    // Parser emits CapabilityCall { name: "obj.method", args } for this.
                    // Only split when the receiver (obj) is a declared variable —
                    // dotted names like "net.fetch" are namespaced capabilities.
                    let is_method_call = name.find('.')
                        .map(|pos| self.declared_vars.contains(&name[..pos]))
                        .unwrap_or(false);
                    if is_method_call {
                        let dot_pos = name.find('.').unwrap();
                        let obj_name = &name[..dot_pos];
                        let method = &name[dot_pos + 1..];
                        self.all_permissions.insert(method.to_string());
                        instrs.push(self.create_instr("load", serde_json::json!({"name": obj_name}), &meta));
                        for arg in args.iter().rev() {
                            self.compile_expr(arg, instrs)?;
                        }
                        instrs.push(self.create_instr("cap_call", serde_json::json!({"name": method, "argc": args.len() + 1}), &meta));
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
                // Compile args first — they'll be pushed onto the stack
                // before the function name. The scheduler will pop argc
                // args + fn_name, then create a new thread with args
                // pre-loaded on the stack.
                for arg in args {
                    self.compile_expr(arg, instrs)?;
                }
                instrs.push(self.create_instr(
                    "push_str",
                    serde_json::json!({"value": function}),
                    meta,
                ));
                instrs.push(self.create_instr(
                    "spawn",
                    serde_json::json!({"argc": args.len()}),
                    meta,
                ));
            }
            Expression::ArrayLiteral { elements, meta } => {
                // Same bug shape (and same fix) as `Expression::ObjectLiteral`
                // just above: ARR_PUSH's contract (crush-vm/src/scheduler.rs)
                // is pop value, pop array, push element, *push the array back*
                // — the re-push already carries the array forward, so the
                // `dup` here was leaving one extra stray array reference on
                // the stack per element (found via the same nested-ArrayLiteral
                // exception `args` field that surfaced the ObjectLiteral bug).
                instrs.push(self.create_instr(
                    "new_array",
                    serde_json::json!({"size": elements.len()}),
                    meta,
                ));

                for element in elements {
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
                // Create a new object/map, then set each property in turn.
                //
                // SET_FIELD's contract (crush-vm/src/scheduler.rs) is: pop
                // value, pop map, insert, *push the map back*. That re-push
                // already carries the map forward to the next property —
                // an extra `dup` before each property's value used to be
                // pushed here too, which left one extra stray copy of the
                // map on the stack per property (found while wiring
                // CRUSHAST-PYLOWER-1's `raise`-value object literals: a
                // 3-property object leaked 3 uncollected Map references,
                // invisible for single-property objects/no downstream
                // type-checked pop, but corrupting later type-checked pops
                // — e.g. THROW's `error_var` binding — once anything with
                // >1 property or a nested Array/Object value was built).
                // No `dup` needed: stack stays exactly `[map]` throughout.
                instrs.push(self.create_instr("new_obj", serde_json::json!({}), meta));
                for (key, value) in properties {
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

                let hints = extract_type_hints(params, body);
                self.lambdas.insert(
                    lambda_name.clone(),
                    CasmFunction {
                        params: params.iter().map(|(n, _)| n.clone()).collect(),
                        locals: vec![],
                        type_hints: if hints.is_empty() { None } else { Some(hints) },
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
            AIExpression::SemanticMatch {
                target,
                concept,
                confidence_threshold,
            } => {
                self.compile_expr(target, instrs)?;
                instrs.push(self.create_instr(
                    "ai_semantic_match",
                    serde_json::json!({
                        "concept": concept,
                        "confidence_threshold": confidence_threshold
                    }),
                    meta,
                ));
            }
            AIExpression::Synthesize {
                output_type,
                constraints,
                context_refs,
                examples,
            } => {
                // Compile context refs
                for expr in context_refs {
                    self.compile_expr(expr, instrs)?;
                }
                if let Some(exs) = examples {
                    for expr in exs {
                        self.compile_expr(expr, instrs)?;
                    }
                }
                
                instrs.push(self.create_instr(
                    "ai_synthesize",
                    serde_json::json!({
                        "output_type": format!("{:?}", output_type),
                        "constraints": constraints
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

fn extract_type_hints(params: &[(String, CastType)], body: &[Statement]) -> HashMap<String, String> {
    let mut hints = HashMap::new();
    for (name, ty) in params {
        if !matches!(ty, CastType::Any) {
            hints.insert(name.clone(), ty.to_string());
        }
    }
    fn scan_stmts(stmts: &[Statement], hints: &mut HashMap<String, String>) {
        for stmt in stmts {
            match stmt {
                Statement::VarDecl { name, type_hint, .. } => {
                    if !matches!(type_hint, CastType::Any) {
                        hints.insert(name.clone(), type_hint.to_string());
                    }
                }
                Statement::If { then_body, else_body, .. } => {
                    scan_stmts(then_body, hints);
                    if let Some(eb) = else_body {
                        scan_stmts(eb, hints);
                    }
                }
                Statement::While { body, .. } => scan_stmts(body, hints),
                Statement::For { body, .. } => scan_stmts(body, hints),
                Statement::TryCatch { body, handler, .. } => {
                    scan_stmts(body, hints);
                    scan_stmts(handler, hints);
                }
                // FunctionDef acts as a new scope boundary; we don't bleed local types from it
                _ => {}
            }
        }
    }
    scan_stmts(body, &mut hints);
    hints
}
