use anyhow::{Result, bail};
use casm::debug_info::{DebugInfo, SourceLocation};
use casm::{Function as CasmFunction, Instruction, Manifest, OpCode, Program as CasmProgram};
use crush_cast::*;
use std::collections::{HashMap, HashSet};

/// Device-side GPU intrinsics lowered to `ptx_*` CASM ops (CRUSH-zorro-3). Host-side
/// `gpu.launch` is deliberately NOT here: it stays an ordinary capability call.
const GPU_INTRINSICS: &[&str] = &[
    "gpu.thread_idx",
    "gpu.block_idx",
    "gpu.block_dim",
    "gpu.global_id",
    "gpu.load_f32",
    "gpu.store_f32",
    "gpu.f32",
];

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
    /// While compiling a lambda body: the enclosing scopes' variables. Lambdas are
    /// lifted to plain functions with no environment, so touching one of these
    /// from inside a lambda is a capture we can't honour yet (CRUSH-251) — it is
    /// reported at compile time instead of faulting at run time.
    lambda_outer_vars: Vec<HashSet<String>>,
    /// CRUSH-251 step 1: `let f = |x| ..` binds `f` to a uniquely lifted
    /// `__lambda_N_f` in the current scope (one map per function / lambda
    /// body, innermost last). Lambdas used to be lifted *as* `f`, so two
    /// functions' `let h = ..` lambdas overwrote each other and a `let f`
    /// lambda replaced a top-level `fn f` program-wide. `None` is a tombstone:
    /// `f` is bound to something else in that scope (`let f = 5`, a parameter
    /// named `f`), so an outer alias must not be found through it.
    lambda_aliases: Vec<HashMap<String, Option<String>>>,
    /// Program-wide counter for unique lambda names.
    lambda_counter: usize,
    /// Names bound more than once in the top-level function being compiled
    /// (see `rebound_names`): a lambda bound to one gets no alias.
    rebound: HashSet<String>,
    /// Arity of every program function (top-level and nested defs), for
    /// `make_closure` when a function's name is used as a value.
    function_arity: HashMap<String, usize>,
    /// CRUSH-251 step 3: variables of the function being compiled that some
    /// closure captures by reference (see `boxed_names`). Their slots hold a
    /// cell; loads and stores go through it (`load_cell` / `store_cell`).
    boxed: HashSet<String>,
    /// Names each enclosing scope binds (function, then lambda bodies,
    /// innermost last) — what a lambda can capture.
    scopes: Vec<HashSet<String>>,
    /// COSM K1.2 (CRUSH-zorro-41): the module the last compiled program's
    /// `struct` / `impl` / `trait` declarations lowered to.
    cosm: Option<crush_cosm::CosmModule>,
    /// Names of methods (with `self`) defined in the program's `impl`s. A
    /// `x.m(…)` call naming one is rejected until receiver dispatch lands
    /// (K1.4/K1.5) — compiled today it would silently call the global
    /// capability `m`.
    receiver_methods: std::collections::BTreeSet<String>,
}

struct LoopInfo {
    continue_target: usize,
    continue_indices: Vec<usize>,
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
            lambda_outer_vars: Vec::new(),
            lambda_aliases: Vec::new(),
            lambda_counter: 0,
            rebound: HashSet::new(),
            function_arity: HashMap::new(),
            boxed: HashSet::new(),
            scopes: Vec::new(),
            cosm: None,
            receiver_methods: Default::default(),
        }
    }

    /// The COSM module of the last compiled program (COSM K1.2): `None` when
    /// it declares no struct, impl or trait.
    pub fn cosm_module(&self) -> Option<&crush_cosm::CosmModule> {
        self.cosm.as_ref()
    }

    /// Compile a CAST program whose `impl` / `trait` declarations
    /// [`crate::cosm_lower::lower`] has already lowered to `cosm` (methods
    /// lifted into `program.functions`).
    pub fn compile_lowered(
        &mut self,
        program: Program,
        cosm: Option<crush_cosm::CosmModule>,
    ) -> Result<CasmProgram> {
        if let Some(module) = &cosm {
            crate::cosm_lower::refuse_unimplemented_strategies(module)?;
        }
        self.receiver_methods =
            cosm.as_ref().map(crate::cosm_lower::receiver_methods).unwrap_or_default();
        self.cosm = cosm;
        self.compile_program(program)
    }

    /// #78: a lambda body may not read or assign the enclosing function's
    /// variables — lambdas don't capture yet (CRUSH-251).
    /// CRUSH-251 step 1: a `let`-bound lambda is a compile-time alias, resolved
    /// in source order — so reassigning it (in a loop, a branch, or even
    /// straight-line) would make calls silently use the wrong lambda. Until
    /// function values exist (step 2) that is a loud error.
    fn reject_lambda_alias_reassignment(&self, name: &str) -> Result<()> {
        if self.resolve_lambda_alias(name).is_some() {
            bail!(
                "reassigning `{name}`, which is bound to a lambda, needs function values \
                 (CRUSH-251 step 2); bind the new lambda to a different name"
            );
        }
        Ok(())
    }

    /// Scope entry for boxed variables (CRUSH-251 step 3): a boxed parameter's
    /// incoming value is wrapped in a cell, a boxed local starts as a cell
    /// holding null. These stores are `raw` (they write the cell itself).
    fn emit_cell_init(
        &self,
        params: &[String],
        locals: &HashSet<String>,
        instrs: &mut Vec<Instruction>,
        meta: &HashMap<String, serde_json::Value>,
    ) {
        for p in params.iter().filter(|p| self.boxed.contains(*p)) {
            instrs.push(self.create_instr("load", serde_json::json!({"name": p, "raw": true}), meta));
            instrs.push(self.create_instr("cell_new", serde_json::json!({}), meta));
            instrs.push(self.create_instr("store", serde_json::json!({"name": p, "raw": true}), meta));
        }
        let mut boxed_locals: Vec<&String> = locals
            .iter()
            .filter(|l| self.boxed.contains(*l) && !params.contains(*l))
            .collect();
        boxed_locals.sort();
        for l in boxed_locals {
            instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
            instrs.push(self.create_instr("cell_new", serde_json::json!({}), meta));
            instrs.push(self.create_instr("store", serde_json::json!({"name": l, "raw": true}), meta));
        }
    }

    /// Route every load / store of a boxed variable from `from` on through its
    /// cell. Renaming in place keeps instruction indices (jump targets) valid.
    fn rewrite_cells(&self, instrs: &mut [Instruction], from: usize) {
        if self.boxed.is_empty() {
            return;
        }
        for instr in &mut instrs[from..] {
            let raw = instr.args.get("raw").and_then(|v| v.as_bool()).unwrap_or(false);
            let boxed = instr.args.get("name").and_then(|v| v.as_str()).is_some_and(|n| self.boxed.contains(n));
            if raw || !boxed {
                continue;
            }
            match instr.op.as_str() {
                "load" => instr.op = "load_cell".to_string(),
                "store" => instr.op = "store_cell".to_string(),
                _ => {}
            }
        }
    }

    /// Set up a function-level scope (a top-level fn or a nested `def`):
    /// boxed variables, the scope's names, cell init after the param stores.
    /// Returns where the body starts (for `rewrite_cells`).
    fn enter_function_scope(
        &mut self,
        params: &[(String, CastType)],
        body: &[Statement],
        instrs: &mut Vec<Instruction>,
        meta: &HashMap<String, serde_json::Value>,
    ) -> usize {
        let pnames: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let body_json = serde_json::to_value(body).unwrap_or_default();
        self.boxed = boxed_names(&pnames, &body_json);
        let mut level = Level::default();
        scan_level(&body_json, &mut level);
        let locals: HashSet<String> = level.decls.union(&level.assigns).cloned().collect();
        self.scopes = vec![pnames.iter().cloned().chain(locals.iter().cloned()).collect()];
        self.emit_cell_init(&pnames, &locals, instrs, meta);
        instrs.len()
    }

    /// The lifted name a `let`-bound lambda alias resolves to, innermost scope first.
    fn resolve_lambda_alias(&self, name: &str) -> Option<String> {
        self.lambda_aliases
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .flatten()
    }

    fn check_lambda_capture(&self, name: &str) -> Result<()> {
        if !self.lambda_outer_vars.is_empty()
            && !self.declared_vars.contains(name)
            && !self.local_functions.contains(name)
            && self.resolve_lambda_alias(name).is_none()
            && self.lambda_outer_vars.iter().any(|outer| outer.contains(name))
        {
            bail!(
                "lambda uses `{name}` from the enclosing function; lambdas can't capture \
                 variables yet (CRUSH-251) — pass `{name}` in as a parameter"
            );
        }
        Ok(())
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

    /// Compile a CAST program to CASM. `impl` / `trait` declarations are
    /// lowered to COSM first (see [`Compiler::cosm_module`]).
    pub fn compile(&mut self, mut program: Program) -> Result<CasmProgram> {
        let cosm = crate::cosm_lower::lower(&mut program)?;
        self.compile_lowered(program, cosm)
    }

    fn compile_program(&mut self, mut program: Program) -> Result<CasmProgram> {
        self.local_functions.clear();
        self.lambdas.clear();
        self.lambda_aliases.clear();
        self.lambda_counter = 0;
        self.function_arity.clear();
        for (name, func) in &program.functions {
            self.local_functions.insert(name.clone());
            self.function_arity.insert(name.clone(), func.params.len());
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
                    Statement::FunctionDef { name, params, .. } => {
                        self.local_functions.insert(name.clone());
                        self.function_arity.insert(name.clone(), params.len());
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
        let mut casm_program = CasmProgram::new();
        casm_program.version = program.cast_version;
        casm_program.lang = program.lang;
        // COSM K1.3 (CRUSH-zorro-42): the kind table travels with the program.
        casm_program.cosm = self.cosm.clone();

        for (name, func) in program.functions {
            let mut instrs = Vec::new();
            self.lambda_aliases.push(HashMap::new());
            self.rebound = rebound_names(&func.params, &func.body);

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
                    self.lambda_aliases
                        .push(params.iter().map(|(n, _)| (n.clone(), None)).collect());
                    let body_start = self.enter_function_scope(params, body, &mut func_instrs, meta);
                    let nested: Result<()> = body
                        .iter()
                        .try_for_each(|inner_stmt| self.compile_stmt(inner_stmt, &mut func_instrs));
                    self.lambda_aliases.pop();
                    nested?;
                    self.rewrite_cells(&mut func_instrs, body_start);
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
            let body_start = if func.is_async {
                0
            } else {
                self.enter_function_scope(&func.params, &func.body, &mut instrs, &func.meta)
            };

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
                let inner_start =
                    self.enter_function_scope(&func.params, &func.body, &mut inner_instrs, &func.meta);

                for stmt in &func.body {
                    if !matches!(stmt, Statement::FunctionDef { .. }) {
                        self.compile_stmt(stmt, &mut inner_instrs)?;
                    }
                }
                self.rewrite_cells(&mut inner_instrs, inner_start);

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

                // Last-first, as for a call (see `Expression::Spawn`).
                for (param_name, _) in func.params.iter().rev() {
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
                self.rewrite_cells(&mut instrs, body_start);

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
            self.lambda_aliases.pop();
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
        let fn_start = debug_info.source_map.len();

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

        // Record function range for correct multi-function pc→location lookup (CRUSH-79).
        let fn_end = debug_info.source_map.len();
        debug_info.record_function_range(function_name, fn_start, fn_end);
    }

    fn ensure_return(
        &self,
        instrs: &mut Vec<Instruction>,
        meta: Option<&HashMap<String, serde_json::Value>>,
    ) {
        // The end of the body is reachable unless the last instruction never
        // falls through and nothing jumps past it. Checking for "any `ret`"
        // missed `fn f(c) { if c { return } ... }`, which then ran off the end
        // of the function (CRUSH-187, GitHub #37 / #92).
        let len = instrs.len() as u64;
        let ends_in_terminator = instrs
            .last()
            .is_some_and(|i| matches!(i.op.as_str(), "ret" | "throw" | "jmp" | "halt"));
        let jumps_past_end = instrs
            .iter()
            .any(|i| i.args.get("target").and_then(|t| t.as_u64()).is_some_and(|t| t >= len));
        if !ends_in_terminator || jumps_past_end {
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

                self.loop_stack.push(LoopInfo {
                    continue_target: 0,
                    continue_indices: Vec::new(),
                    break_indices: Vec::new(),
                });

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
                let loop_info = self.loop_stack.last_mut().unwrap();
                for idx in loop_info.continue_indices.drain(..) {
                    instrs[idx].args = serde_json::json!({"target": continue_idx});
                }
                loop_info.continue_target = continue_idx;

                instrs.push(self.create_instr("load", serde_json::json!({"name": variable}), meta));
                instrs.push(self.create_instr("push_int", serde_json::json!({"value": 1}), meta));
                instrs.push(self.create_instr("add", serde_json::json!({}), meta));
                instrs.push(self.create_instr("store", serde_json::json!({"name": variable}), meta));

                instrs.push(self.create_instr("jmp", serde_json::json!({"target": loop_start}), meta));

                let loop_end = instrs.len();
                instrs[jmp_if_not_idx].args = serde_json::json!({"target": loop_end});
                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.continue_indices {
                    instrs[idx].args = serde_json::json!({"target": loop_start});
                }
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
                // `let f = 5` after `let f = |x| ..`: `f` is no longer the lambda,
                // so a later `f(..)` must not call it (a new lambda re-adds it).
                if !matches!(value, Expression::Lambda { .. }) {
                    if let Some(scope) = self.lambda_aliases.last_mut() {
                        scope.insert(name.clone(), None);
                    }
                }
                // A Crush `let` — or a walker declaration marked as a block
                // binding (JS `let` / `const`) — is a fresh binding each time
                // it runs (in a loop body: one per iteration), so closures
                // made in different iterations don't share it. The cell is
                // made before the initializer, so a lambda that refers to its
                // own name captures this binding. Other walker declarations
                // keep one binding: Python variables and JS `var` are
                // function-scoped.
                let block_binding = !meta.contains_key("lang")
                    || meta.get("binding").and_then(|b| b.as_str()) == Some("block");
                if self.boxed.contains(name.as_str()) && block_binding {
                    instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
                    instrs.push(self.create_instr("cell_new", serde_json::json!({}), meta));
                    instrs.push(self.create_instr("store", serde_json::json!({"name": name, "raw": true}), meta));
                }
                self.compile_expr_with_name_hint(value, instrs, Some(name))?;
                instrs.push(self.create_typed_instr(OpCode::Store(name.clone()), meta)?);
            }
            Statement::Assign {
                target, value, meta
            } => {
                self.reject_lambda_alias_reassignment(target)?;
                self.check_lambda_capture(target)?;
                self.declared_vars.insert(target.clone());
                self.compile_expr_with_name_hint(value, instrs, Some(target))?;
                instrs.push(self.create_typed_instr(OpCode::Store(target.clone()), meta)?);
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
                    instrs.push(self.create_typed_instr(OpCode::Pop, meta)?);
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
                    continue_indices: Vec::new(),
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

                // Patch continue/break jumps
                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.continue_indices {
                    instrs[idx].args = serde_json::json!({"target": loop_start});
                }
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
                // Convert maps to key arrays (arrays/strings pass through unchanged)
                instrs.push(self.create_instr("map_keys", serde_json::json!({}), meta));
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

                // Track continue jumps (same patching pattern as break_indices)
                // because we don't know the final increment position yet.
                self.loop_stack.push(LoopInfo {
                    continue_target: 0,
                    continue_indices: Vec::new(),
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
                // Patch all continue jumps that were emitted during body compilation
                let loop_info = self.loop_stack.last_mut().unwrap();
                for idx in loop_info.continue_indices.drain(..) {
                    instrs[idx].args = serde_json::json!({"target": continue_idx});
                }
                loop_info.continue_target = continue_idx;

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
                // SET_FIELD pushes the map back; as a statement, drop it.
                instrs.push(self.create_instr("pop", serde_json::json!({}), meta));
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
                imports,
                deps,
                meta,
            } => {
                // CRUSH-224: each block runs in a fresh interpreter, so the `use @lang`
                // imports that reach it are written into its own source.
                let code = match crate::lang_imports::import_header(lang, imports)
                    .map_err(|e| anyhow::anyhow!(e))?
                {
                    Some(header) => crate::lang_imports::splice_header(lang, code, &header),
                    None => code.clone(),
                };
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
                    // CRUSH-18: carry the `.crush`-source line of this
                    // `@lang { ... }` block into the compiled spec so a
                    // guest runtime error can be tied back to it (see
                    // `VmError::LangRuntimeError`, `parse_lang_block`).
                    if let Some(line) = meta.get("line").and_then(|v| v.as_u64()) {
                        map.insert("crush_line".to_string(), serde_json::json!(line));
                    }
                    // CRUSH-20: `@lang[dep1, dep2]` bare-runtime buckets
                    // package specs, threaded through to EXEC_LANG's
                    // sandboxed-provisioning path. Omitted entirely (not an
                    // empty array) when there are none, so the unsandboxed
                    // opcode handler's `unwrap_or_default` reads identically
                    // to before this field existed.
                    if !deps.is_empty() {
                        map.insert("deps".to_string(), serde_json::json!(deps));
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
                        instrs.push(self.create_typed_instr(OpCode::Pop, meta)?);
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
                if self.loop_stack.is_empty() {
                    bail!("continue outside of loop");
                }
                let idx = instrs.len();
                instrs.push(self.create_instr(
                    "jmp",
                    serde_json::json!({"target": 0}),
                    meta,
                ));
                self.loop_stack.last_mut().unwrap().continue_indices.push(idx);
            }
            Statement::StructDef { .. } => {}
            // Lowered to COSM + lifted functions before code generation
            // (`cosm_lower`); reaching here means an `impl`/`trait` outside
            // the top level of a file.
            Statement::ImplBlock { meta, .. } | Statement::TraitDef { meta, .. } => {
                let at = match (meta.get("line").and_then(|v| v.as_u64()), meta.get("col").and_then(|v| v.as_u64())) {
                    (Some(l), Some(c)) => format!("line {l}, col {c}: "),
                    _ => String::new(),
                };
                bail!("{at}`impl` / `trait` blocks are only allowed at the top level of a file");
            }
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
                    ImportStatement::PolyglotModule { language, .. } => {
                        // Emits nothing here: there is no interpreter session to load into.
                        // The import is written into each later `@lang` block instead
                        // (`LangBlock.imports`, filled by crush-lang-sdk's polyglot pass,
                        // see `lang_imports`). CRUSH-224.
                        crate::lang_imports::check_polyglot_use(language)
                            .map_err(|e| anyhow::anyhow!(e))?;
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

    /// `a && b` / `a || b` evaluate `b` only when `a` doesn't decide the
    /// result (CRUSH-125). Both still produce a bool, as the eager `and`/`or`
    /// opcodes did. Only `jmp_if_not` / `jmp` are used — the same branches
    /// `if` compiles to, so every backend already supports them:
    ///
    /// ```text
    /// &&:  <a> jmp_if_not F  <b> jmp_if_not F  push true  jmp E  F: push false  E:
    /// ||:  <a> jmp_if_not B  jmp T  B: <b> jmp_if_not F  T: push true  jmp E  F: push false  E:
    /// ```
    fn compile_short_circuit(
        &mut self,
        operator: &str,
        left: &Expression,
        right: &Expression,
        meta: &HashMap<String, serde_json::Value>,
        instrs: &mut Vec<Instruction>,
    ) -> Result<()> {
        let is_and = matches!(operator, "and" | "&&");
        let mut to_false = Vec::new();
        let mut to_true = Vec::new();

        self.compile_expr(left, instrs)?;
        let left_falsy = instrs.len();
        instrs.push(self.create_instr("jmp_if_not", serde_json::json!({"target": 0}), meta));
        if is_and {
            to_false.push(left_falsy);
        } else {
            to_true.push(instrs.len());
            instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), meta));
            let eval_right = instrs.len();
            instrs[left_falsy].args = serde_json::json!({"target": eval_right});
        }

        self.compile_expr(right, instrs)?;
        to_false.push(instrs.len());
        instrs.push(self.create_instr("jmp_if_not", serde_json::json!({"target": 0}), meta));

        let true_label = instrs.len();
        instrs.push(self.create_typed_instr(OpCode::PushBool(true), meta)?);
        let to_end = instrs.len();
        instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), meta));
        let false_label = instrs.len();
        instrs.push(self.create_typed_instr(OpCode::PushBool(false), meta)?);
        let end_label = instrs.len();

        for idx in to_false {
            instrs[idx].args = serde_json::json!({"target": false_label});
        }
        for idx in to_true {
            instrs[idx].args = serde_json::json!({"target": true_label});
        }
        instrs[to_end].args = serde_json::json!({"target": end_label});
        Ok(())
    }

    fn compile_expr_with_name_hint(
        &mut self,
        expr: &Expression,
        instrs: &mut Vec<Instruction>,
        name_hint: Option<&str>,
    ) -> Result<()> {
        match expr {
            Expression::IntLiteral { value, meta } => {
                instrs.push(self.create_typed_instr(OpCode::PushInt(*value), meta)?);
            }
            Expression::FloatLiteral { value, meta } => {
                instrs.push(self.create_typed_instr(OpCode::PushFloat(*value), meta)?);
            }
            Expression::StringLiteral { value, meta } => {
                instrs.push(self.create_typed_instr(OpCode::PushStr(value.clone()), meta)?);
            }
            Expression::BoolLiteral { value, meta } => {
                instrs.push(self.create_typed_instr(OpCode::PushBool(*value), meta)?);
            }
            Expression::NullLiteral { meta } => {
                instrs.push(self.create_typed_instr(OpCode::PushNull, meta)?);
            }
            Expression::Var { name, meta } => {
                let outer = self.lambda_outer_vars.iter().any(|o| o.contains(name.as_str()));
                match self.function_arity.get(name.as_str()) {
                    // A function's name used as a value (`apply(fib, 3)`).
                    Some(&arity) if !self.declared_vars.contains(name.as_str()) && !outer => {
                        instrs.push(self.create_instr(
                            "make_closure",
                            serde_json::json!({"function": name, "arity": arity, "ncap": 0}),
                            meta,
                        ));
                    }
                    _ => {
                        self.check_lambda_capture(name)?;
                        instrs.push(self.create_typed_instr(OpCode::Load(name.clone()), meta)?);
                    }
                }
            }
            Expression::BinaryOp {
                operator,
                left,
                right,
                meta,
            } => {
                if matches!(operator.as_str(), "and" | "&&" | "or" | "||") {
                    return self.compile_short_circuit(operator, left, right, meta, instrs);
                }
                // `a ** b`: exact for int ** non-negative int, float otherwise
                // (one shared implementation in every engine; `math_pow` is
                // always float).
                if operator == "**" {
                    self.all_permissions.insert("math.power".to_string());
                    self.compile_expr(left, instrs)?;
                    self.compile_expr(right, instrs)?;
                    instrs.push(self.create_instr(
                        "cap_call",
                        serde_json::json!({"name": "math.power", "argc": 2}),
                        meta,
                    ));
                    return Ok(());
                }
                self.compile_expr(left, instrs)?;
                self.compile_expr(right, instrs)?;
                let opcode = match operator.as_str() {
                    "+" => OpCode::Add,
                    "-" => OpCode::Sub,
                    "*" => OpCode::Mul,
                    "/" | "//" => OpCode::Div,
                    "%" => OpCode::Mod,
                    "===" | "==" => OpCode::Eq,
                    "!==" | "!=" => OpCode::Ne,
                    "<" => OpCode::Lt,
                    ">" => OpCode::Gt,
                    "<=" => OpCode::Le,
                    ">=" => OpCode::Ge,
                    _ => bail!("Unsupported op: {}", operator),
                };
                instrs.push(self.create_typed_instr(opcode, meta)?);
            }
            Expression::UnaryOp {
                operator,
                operand,
                meta,
            } => {
                self.compile_expr(operand, instrs)?;
                let opcode = match operator.as_str() {
                    "-" => OpCode::Neg,
                    "not" | "!" => {
                        instrs.push(self.create_instr("not", serde_json::json!({}), meta));
                        return Ok(());
                    }
                    _ => bail!("Unsupported op: {}", operator),
                };
                instrs.push(self.create_typed_instr(opcode, meta)?);
            }
            Expression::Call {
                function,
                args,
                meta,
            } => {
                // #78: `f(x)` where `f` is a variable (a lambda passed in as an
                // argument, say) would otherwise compile to a capability call
                // named "f". It holds a function value: call it through the
                // value (CRUSH-251). A `let`-bound lambda alias in scope is
                // called directly.
                let aliased = self.resolve_lambda_alias(function);
                if aliased.is_none() && !function.contains('.') {
                    // `f` from an enclosing function would be a capture (step 3).
                    self.check_lambda_capture(function)?;
                }
                let function: &String = aliased.as_ref().unwrap_or(function);
                if aliased.is_none()
                    && !function.contains('.')
                    && self.declared_vars.contains(function.as_str())
                {
                    // Args last-first (as for `call`), then the callee on top.
                    for arg in args.iter().rev() {
                        self.compile_expr(arg, instrs)?;
                    }
                    instrs.push(self.create_typed_instr(OpCode::Load(function.clone()), meta)?);
                    instrs.push(self.create_instr(
                        "call_value",
                        serde_json::json!({"argc": args.len()}),
                        meta,
                    ));
                } else if function == "len" {
                    if args.len() != 1 {
                        bail!("len() expects exactly 1 argument");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("len", serde_json::json!({}), meta));
                } else if function == "parallel_map" && !self.local_functions.contains("parallel_map") {
                    // #175: a VM capability (it starts workers); capabilities
                    // take their arguments in order.
                    if args.len() != 2 {
                        bail!("parallel_map(items, f) expects exactly 2 arguments");
                    }
                    for arg in args {
                        self.compile_expr(arg, instrs)?;
                    }
                    self.all_permissions.insert("parallel_map".to_string());
                    instrs.push(self.create_instr(
                        "cap_call",
                        serde_json::json!({"name": "parallel_map", "argc": 2}),
                        meta,
                    ));
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
                    // The `array_pop` op leaves [array, value]; `array.pop(a)` is
                    // an expression whose value is the element, so drop the
                    // array — otherwise it shifts the operands of whatever
                    // encloses the call (`print(len(a), a.pop())` printed the array).
                    instrs.push(self.create_instr("array_pop", serde_json::json!({}), meta));
                    instrs.push(self.create_instr("swap", serde_json::json!({}), meta));
                    instrs.push(self.create_instr("pop", serde_json::json!({}), meta));
                } else if function == "__crush_slice__" {
                    // `t[start:stop:step]` (null bounds = from the edge).
                    if args.len() != 4 {
                        bail!("slice as a value (outside a subscript) is not supported yet");
                    }
                    self.all_permissions.insert("seq.slice".to_string());
                    for a in args {
                        self.compile_expr(a, instrs)?;
                    }
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "seq.slice", "argc": 4}), &meta));
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
                } else if function == "__crush_rebind__" {
                    // A fresh binding holding the variable's current value
                    // (JS `for (let ..)` per-iteration copy, CRUSH-251 step
                    // 4b): closures made so far keep the old binding. Only a
                    // boxed (captured) variable has a binding to renew.
                    let Some(Expression::Var { name, .. }) = args.first() else {
                        bail!("__crush_rebind__ expects a variable");
                    };
                    if self.boxed.contains(name.as_str()) {
                        instrs.push(self.create_instr("load_cell", serde_json::json!({"name": name}), meta));
                        instrs.push(self.create_instr("cell_new", serde_json::json!({}), meta));
                        instrs.push(self.create_instr("store", serde_json::json!({"name": name, "raw": true}), meta));
                    }
                    instrs.push(self.create_instr("push_null", serde_json::json!({}), meta));
                } else if function == "__crush_assign__" {
                    // Intrinsic: assignment lowered from walker (e.g., `x = 42` in JS)
                    // args[0] = target Var, args[1] = value expression
                    if args.len() != 2 {
                        bail!("__crush_assign__ expects exactly 2 arguments");
                    }
                    if let Expression::Var { name, .. } = &args[0] {
                        self.reject_lambda_alias_reassignment(name)?;
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
                } else if function == "__crush_js_or__" || function == "__crush_js_and__" {
                    // JS `a || b` / `a && b`: the deciding operand itself, by JS
                    // truthiness (`js.truthy`); `b` runs only when `a` doesn't
                    // decide. Crush's own `||`/`&&` (compile_short_circuit) stay
                    // boolean.
                    //   ||:  <a> dup js.truthy jmp_if_not R  jmp E  R: pop <b>  E:
                    //   &&:  <a> dup js.truthy jmp_if_not E  pop <b>  E:
                    if args.len() != 2 { bail!("{function} expects exactly 2 arguments"); }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_typed_instr(OpCode::Dup, meta)?);
                    self.all_permissions.insert("js.truthy".to_string());
                    instrs.push(self.create_instr("cap_call", serde_json::json!({"name": "js.truthy", "argc": 1}), meta));
                    let test_idx = instrs.len();
                    instrs.push(self.create_instr("jmp_if_not", serde_json::json!({"target": 0}), meta));
                    let mut to_end = Vec::new();
                    if function == "__crush_js_or__" {
                        to_end.push(instrs.len());
                        instrs.push(self.create_instr("jmp", serde_json::json!({"target": 0}), meta));
                        let rhs = instrs.len();
                        instrs[test_idx].args = serde_json::json!({"target": rhs});
                    } else {
                        to_end.push(test_idx);
                    }
                    instrs.push(self.create_typed_instr(OpCode::Pop, meta)?);
                    self.compile_expr(&args[1], instrs)?;
                    let end_label = instrs.len();
                    for idx in to_end {
                        instrs[idx].args = serde_json::json!({"target": end_label});
                    }
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
                        instrs.push(self.create_typed_instr(OpCode::Dup, &meta)?);
                    }
                    instrs.push(self.create_instr("push_int", serde_json::json!({"value": 1}), &meta));
                    if is_inc {
                        instrs.push(self.create_instr("add", serde_json::json!({}), &meta));
                    } else {
                        instrs.push(self.create_instr("sub", serde_json::json!({}), &meta));
                    }
                    if is_pre {
                        instrs.push(self.create_typed_instr(OpCode::Dup, &meta)?);
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
                } else if function == "ai_synthesize" {
                    // ai_synthesize(type_str [, constraints=[...]]) — lower to ai_synthesize CASM
                    let type_str = if let Some(Expression::StringLiteral { value, .. }) = args.first() {
                        value.clone()
                    } else {
                        bail!("ai_synthesize: first argument must be a string literal (output type)");
                    };
                    let constraints: Vec<String> = if args.len() > 1 {
                        if let Expression::ArrayLiteral { elements, .. } = &args[1] {
                            elements.iter().filter_map(|e| {
                                if let Expression::StringLiteral { value, .. } = e {
                                    Some(value.clone())
                                } else {
                                    None
                                }
                            }).collect()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };
                    instrs.push(self.create_instr(
                        "ai_synthesize",
                        serde_json::json!({
                            "output_type": type_str,
                            "constraints": constraints
                        }),
                        &meta,
                    ));
                } else if function == "ai_semantic_match" {
                    // ai_semantic_match(target, concept_str [, threshold]) — lower to ai_semantic_match CASM
                    if args.len() < 2 {
                        bail!("ai_semantic_match: expects at least 2 arguments (target, concept)");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    let concept = if let Expression::StringLiteral { value, .. } = &args[1] {
                        value.clone()
                    } else {
                        bail!("ai_semantic_match: second argument must be a string literal (concept)");
                    };
                    let threshold = if let Some(Expression::FloatLiteral { value, .. }) = args.get(2) {
                        *value
                    } else {
                        0.8
                    };
                    instrs.push(self.create_instr(
                        "ai_semantic_match",
                        serde_json::json!({
                            "concept": concept,
                            "confidence_threshold": threshold
                        }),
                        &meta,
                    ));
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
                } else if name == "type_of" {
                    // The CASM `type_of` op (CVM1 `TYPEOF`): the value's runtime
                    // type name. Used by the JS walker's `for-in` guard
                    // (CRUSH-zorro-23). Strings are "str" on the CVM1 VMs and
                    // "string" on FastVM/JIT; an AOT backend without the op
                    // refuses it at compile time.
                    if args.len() != 1 {
                        bail!("type_of() expects exactly 1 argument");
                    }
                    self.compile_expr(&args[0], instrs)?;
                    instrs.push(self.create_instr("type_of", serde_json::json!({}), meta));
                } else if (name == "array.push" || name == "array.pop")
                    && !self.declared_vars.contains("array")
                    && !self.local_functions.contains(name)
                {
                    // The parser emits `array.push(a, x)` / `array.pop(a)` as a
                    // dotted CapabilityCall, not a `Call`, so the `Call`-branch
                    // intrinsics above never saw them and they fell through to
                    // an unknown `cap_call "array.push"`. Lower them to the same
                    // ARR_PUSH / ARR_POP instructions every backend implements
                    // (nanovm's sbl_core.crush relies on both — CRUSH-122).
                    let (op, argc) = if name == "array.push" {
                        ("array_push", 2)
                    } else {
                        ("array_pop", 1)
                    };
                    if args.len() != argc {
                        bail!("{name}() expects exactly {argc} argument(s)");
                    }
                    for arg in args {
                        self.compile_expr(arg, instrs)?;
                    }
                    instrs.push(self.create_instr(op, serde_json::json!({}), meta));
                    if op == "array_pop" {
                        // ARR_POP leaves the array *and* the value (every
                        // backend); the expression's value is just the
                        // element, so drop the array under it. Without this
                        // `let v = array.pop(a)` leaked the array on the stack
                        // (found by CRUSH-142's strict differential test).
                        instrs.push(self.create_instr("swap", serde_json::json!({}), meta));
                        instrs.push(self.create_instr("pop", serde_json::json!({}), meta));
                    }
                } else if GPU_INTRINSICS.contains(&name.as_str())
                    && !self.declared_vars.contains("gpu")
                    && !self.local_functions.contains(name)
                {
                    self.compile_gpu_intrinsic(name, args, meta, instrs)?;
                } else if self.local_functions.contains(name) {
                    // Last-to-first, like every other `call`: the callee's
                    // `store <param1>` pops the first argument (CRUSH-138).
                    for arg in args.iter().rev() {
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
                        if self.receiver_methods.contains(method) {
                            let at = match (
                                meta.get("line").and_then(|v| v.as_u64()),
                                meta.get("col").and_then(|v| v.as_u64()),
                            ) {
                                (Some(l), Some(c)) => format!("line {l}, col {c}: "),
                                _ => String::new(),
                            };
                            bail!(
                                "{at}`{obj_name}.{method}(…)`: calling methods defined in `impl` needs \
                                 COSM K1.4/K1.5 (CRUSH-zorro-43/44) — compiled today it would call the \
                                 global capability `{method}`, not the method. Call the lifted function \
                                 directly instead (`Type::{method}({obj_name}, …)`)"
                            );
                        }
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
                // Args last-first, as for `call`: the task's first `store` pops
                // the first argument. (They were pushed first-first, so a
                // spawned `w(10, 3)` ran as `w(3, 10)`.)
                for arg in args.iter().rev() {
                    self.compile_expr(arg, instrs)?;
                }
                // The task's function is a function value (CRUSH-251 step 5),
                // so SPAWN checks its arity and passes its captured values as
                // leading parameters: a variable holds one (a `let`-bound
                // lambda's variable too); a named function becomes one.
                if self.declared_vars.contains(function.as_str()) {
                    self.check_lambda_capture(function)?;
                    instrs.push(self.create_typed_instr(OpCode::Load(function.clone()), meta)?);
                } else if let Some(&arity) = self.function_arity.get(function.as_str()) {
                    instrs.push(self.create_instr(
                        "make_closure",
                        serde_json::json!({"function": function, "arity": arity, "ncap": 0}),
                        meta,
                    ));
                } else {
                    // Unknown here (an async wrapper, a name only the VM has):
                    // SPAWN resolves it at run time.
                    instrs.push(self.create_instr(
                        "push_str",
                        serde_json::json!({"value": function}),
                        meta,
                    ));
                }
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
            Expression::TupleLiteral { elements, meta } => {
                instrs.push(self.create_instr(
                    "new_tuple",
                    serde_json::json!({"size": elements.len()}),
                    meta,
                ));
                for element in elements {
                    instrs.push(self.create_typed_instr(OpCode::Dup, meta)?);
                    self.compile_expr(element, instrs)?;
                    instrs.push(self.create_instr("tuple_push", serde_json::json!({}), meta));
                }
            }
            Expression::ListLiteral { elements, meta } => {
                instrs.push(self.create_instr(
                    "new_list",
                    serde_json::json!({"size": elements.len()}),
                    meta,
                ));
                for element in elements {
                    instrs.push(self.create_typed_instr(OpCode::Dup, meta)?);
                    self.compile_expr(element, instrs)?;
                    instrs.push(self.create_instr("list_push", serde_json::json!({}), meta));
                }
            }
            Expression::VectorLiteral { elements, meta } => {
                instrs.push(self.create_instr(
                    "new_vector",
                    serde_json::json!({"size": elements.len()}),
                    meta,
                ));
                for element in elements {
                    instrs.push(self.create_typed_instr(OpCode::Dup, meta)?);
                    self.compile_expr(element, instrs)?;
                    instrs.push(self.create_instr("vector_push", serde_json::json!({}), meta));
                }
            }
            Expression::SetLiteral { elements, meta } => {
                instrs.push(self.create_instr(
                    "new_set",
                    serde_json::json!({"size": elements.len()}),
                    meta,
                ));
                for element in elements {
                    instrs.push(self.create_typed_instr(OpCode::Dup, meta)?);
                    self.compile_expr(element, instrs)?;
                    instrs.push(self.create_instr("set_push", serde_json::json!({}), meta));
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
                // Always a unique lifted name; the binding name is only a readable
                // suffix and a scoped alias (see `lambda_aliases`).
                let lambda_name = match name_hint {
                    Some(hint) => format!("__lambda_{}_{hint}", self.lambda_counter),
                    None => format!("__lambda_{}", self.lambda_counter),
                };
                self.lambda_counter += 1;
                // CRUSH-251 step 3: the enclosing variables this lambda uses
                // (transitively, through nested lambdas) become hidden leading
                // parameters; `make_closure` carries their values — the cell
                // for a boxed variable, so writes are shared.
                let pnames: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                let body_json = serde_json::to_value(body).unwrap_or_default();
                let enclosing: HashSet<String> = self.scopes.iter().flatten().cloned().collect();
                let nonlocals = python_nonlocals(serde_json::to_value(meta).ok().as_ref());
                let captures: Vec<String> =
                    lambda_captures(&pnames, &body_json, nonlocals.as_ref(), &enclosing)
                        .into_iter()
                        .collect();
                let mut level = Level::default();
                scan_level(&body_json, &mut level);
                let locals: HashSet<String> =
                    lambda_locals(&[], &level, &enclosing, nonlocals.as_ref());
                // Only a capture-free lambda bound to a name bound exactly once
                // keeps the direct-call alias (a direct `call` can't pass
                // captures); otherwise calls go through the value.
                if let Some(hint) = name_hint
                    .filter(|h| !self.rebound.contains(*h))
                    .filter(|_| captures.is_empty())
                {
                    if self.lambda_aliases.is_empty() {
                        self.lambda_aliases.push(HashMap::new());
                    }
                    self.lambda_aliases.last_mut().expect("scope").insert(hint.to_string(), Some(lambda_name.clone()));
                }

                // Entry: captures, then parameters, popped in that order. These
                // stores are raw: a boxed capture arrives as its cell.
                let mut func_instrs = Vec::new();
                for name in captures.iter().chain(pnames.iter()) {
                    func_instrs.push(self.create_instr(
                        "store",
                        serde_json::json!({"name": name, "raw": true}),
                        meta,
                    ));
                }
                self.emit_cell_init(&pnames, &locals, &mut func_instrs, meta);
                let body_start = func_instrs.len();
                // The lifted function has its own scope: its parameters and
                // captures, plus what its body declares.
                let outer = std::mem::replace(
                    &mut self.declared_vars,
                    pnames.iter().chain(captures.iter()).cloned().collect(),
                );
                self.lambda_outer_vars.push(outer.clone());
                self.lambda_aliases
                    .push(params.iter().map(|(n, _)| (n.clone(), None)).collect());
                self.scopes.push(
                    pnames.iter().chain(captures.iter()).chain(locals.iter()).cloned().collect(),
                );
                let body_result: Result<()> = body
                    .iter()
                    .try_for_each(|inner_stmt| self.compile_stmt(inner_stmt, &mut func_instrs));
                self.scopes.pop();
                self.lambda_aliases.pop();
                self.lambda_outer_vars.pop();
                self.declared_vars = outer;
                body_result?;
                self.ensure_return(&mut func_instrs, Some(meta));
                self.rewrite_cells(&mut func_instrs, body_start);

                let hints = extract_type_hints(params, body);
                self.lambdas.insert(
                    lambda_name.clone(),
                    CasmFunction {
                        params: captures.iter().chain(pnames.iter()).cloned().collect(),
                        locals: vec![],
                        type_hints: if hints.is_empty() { None } else { Some(hints) },
                        body: func_instrs,
                    },
                );

                // The captured values, in order (raw: a boxed variable's cell).
                for name in &captures {
                    instrs.push(self.create_instr("load", serde_json::json!({"name": name, "raw": true}), meta));
                }
                instrs.push(self.create_instr(
                    "make_closure",
                    serde_json::json!({"function": lambda_name, "arity": params.len(), "ncap": captures.len()}),
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
                // CRUSH-156: one tool shape for the chain and its fallbacks
                // (`tool_name`, `parameters`, `result_binding`, `condition`,
                // `required_capability`), read back by the toolchain engine.
                let tool_json = |t: &ToolCall| {
                    serde_json::json!({
                        "tool_name": t.tool_name,
                        "parameters": t.parameters,
                        "result_binding": t.result_binding,
                        "condition": t.condition,
                        "required_capability": t.required_capability
                    })
                };
                let tools_json: Vec<serde_json::Value> = tools.iter().map(tool_json).collect();

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
                            "fallback_count": fallback_tools.len(),
                            "fallback_tools": fallback_tools.iter().map(tool_json).collect::<Vec<_>>()
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
                        "provides_context": provides_context,
                        "stack_args": 1
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
                        "confidence_threshold": confidence_threshold,
                        "stack_args": 1
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
                        "constraints": constraints,
                        "context_refs": context_refs.len(),
                        "stack_args": context_refs.len() + examples.as_ref().map_or(0, |e| e.len())
                    }),
                    meta,
                ));
            }
        }
        Ok(())
    }

    /// Lower a device-side `gpu.*` intrinsic (CRUSH-zorro-3) to the `ptx_*` CASM ops that
    /// crush-ptx compiles. A function using any of these is a GPU kernel: crush-ptx turns
    /// it into PTX, and the CVM1 lowering skips it (it cannot run on the host VM).
    ///
    /// Every integer result is widened to s64, the type crush-ptx gives kernel
    /// parameters and integer literals, so `gpu.global_id() < n` type-checks without a
    /// manual `cvt`. Buffers are f32 device pointers; `gpu.load_f32` / `gpu.store_f32`
    /// take an element index and scale it by 4 themselves.
    fn compile_gpu_intrinsic(
        &mut self,
        name: &str,
        args: &[Expression],
        meta: &HashMap<String, serde_json::Value>,
        instrs: &mut Vec<Instruction>,
    ) -> Result<()> {
        let expect = |n: usize| -> Result<()> {
            if args.len() != n {
                bail!("{name}() expects exactly {n} argument(s), got {}", args.len());
            }
            Ok(())
        };
        let op = |s: &Self, op: &str, args: serde_json::Value| s.create_instr(op, args, meta);
        let s64 = || serde_json::json!({"type": "s64"});
        // `buf + i*4`: the f32 element address, left on the stack.
        let f32_addr = |s: &mut Self, instrs: &mut Vec<Instruction>| -> Result<()> {
            s.compile_expr(&args[0], instrs)?;
            s.compile_expr(&args[1], instrs)?;
            instrs.push(op(s, "push_int", serde_json::json!({"value": 2})));
            instrs.push(op(s, "shl", serde_json::json!({})));
            instrs.push(op(s, "add", serde_json::json!({})));
            Ok(())
        };
        match name {
            "gpu.thread_idx" | "gpu.block_idx" | "gpu.block_dim" => {
                expect(0)?;
                let ptx = match name {
                    "gpu.thread_idx" => "ptx_thread_idx_x",
                    "gpu.block_idx" => "ptx_block_idx_x",
                    _ => "ptx_block_dim_x",
                };
                instrs.push(op(self, ptx, serde_json::json!({})));
                instrs.push(op(self, "cvt", s64()));
            }
            "gpu.global_id" => {
                // block_idx * block_dim + thread_idx (x dimension).
                expect(0)?;
                for ptx in ["ptx_block_idx_x", "ptx_block_dim_x"] {
                    instrs.push(op(self, ptx, serde_json::json!({})));
                    instrs.push(op(self, "cvt", s64()));
                }
                instrs.push(op(self, "mul", serde_json::json!({})));
                instrs.push(op(self, "ptx_thread_idx_x", serde_json::json!({})));
                instrs.push(op(self, "cvt", s64()));
                instrs.push(op(self, "add", serde_json::json!({})));
            }
            "gpu.load_f32" => {
                expect(2)?;
                f32_addr(self, instrs)?;
                instrs.push(op(self, "ptx_ld_global", serde_json::json!({"type": "f32"})));
            }
            "gpu.store_f32" => {
                expect(3)?;
                f32_addr(self, instrs)?;
                self.compile_expr(&args[2], instrs)?;
                instrs.push(op(self, "cvt", serde_json::json!({"type": "f32"})));
                instrs.push(op(self, "ptx_st_global", serde_json::json!({})));
                // A store is a statement; give the expression its usual null value so
                // the `ExprStmt` pop stays balanced.
                instrs.push(op(self, "push_null", serde_json::json!({})));
            }
            "gpu.f32" => {
                expect(1)?;
                self.compile_expr(&args[0], instrs)?;
                instrs.push(op(self, "cvt", serde_json::json!({"type": "f32"})));
            }
            _ => bail!("unknown GPU intrinsic {name}"),
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

    fn create_typed_instr(
        &self,
        opcode: OpCode,
        meta: &HashMap<String, serde_json::Value>,
    ) -> Result<Instruction> {
        let meta_json = serde_json::to_value(meta)?;
        let lang = meta
            .get("lang")
            .and_then(|l| l.as_str())
            .map(|s| s.to_string());
        Ok(Instruction::from_opcode(opcode, lang, Some(meta_json))?)
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

/// CRUSH-251: names bound more than once anywhere in a top-level function —
/// its parameters, `let`s, assignments, loop variables, nested `def` names and
/// parameters, lambda parameters, walker `__crush_assign__` targets. A
/// `let`-bound lambda keeps its direct-call alias only when its name is bound
/// exactly once; otherwise which lambda the name holds depends on control flow
/// (a reassignment in a loop or a branch, a shadowing inner binding), so calls
/// go through the value with `call_value`. Walks the serialized CAST so a new
/// kind of node can't slip past unseen.
fn rebound_names(params: &[(String, CastType)], body: &[Statement]) -> HashSet<String> {
    use serde_json::Value as J;
    fn bind(name: Option<&str>, counts: &mut HashMap<String, usize>) {
        if let Some(n) = name {
            *counts.entry(n.to_string()).or_default() += 1;
        }
    }
    fn walk(v: &J, counts: &mut HashMap<String, usize>) {
        match v {
            J::Array(items) => items.iter().for_each(|i| walk(i, counts)),
            J::Object(o) => {
                let field = |k: &str| o.get(k).and_then(J::as_str);
                match field("type") {
                    Some("VarDecl") => bind(field("name"), counts),
                    Some("Assign") => bind(field("target"), counts),
                    Some("FunctionDef") | Some("Lambda") => {
                        if o.get("type").and_then(J::as_str) == Some("FunctionDef") {
                            bind(field("name"), counts);
                        }
                        for p in o.get("params").and_then(J::as_array).into_iter().flatten() {
                            bind(p.get(0).and_then(J::as_str), counts);
                        }
                    }
                    Some("Call") if field("function") == Some("__crush_assign__") => bind(
                        o.get("args")
                            .and_then(|a| a.get(0))
                            .and_then(|t| t.get("name"))
                            .and_then(J::as_str),
                        counts,
                    ),
                    _ => {}
                }
                bind(field("variable"), counts);
                o.values().for_each(|c| walk(c, counts));
            }
            _ => {}
        }
    }
    let mut counts = HashMap::new();
    for (p, _) in params {
        bind(Some(p), &mut counts);
    }
    walk(&serde_json::to_value(body).unwrap_or_default(), &mut counts);
    counts.into_iter().filter(|(_, n)| *n > 1).map(|(k, _)| k).collect()
}

/// What one scope binds and references at its own level — not inside nested
/// lambdas (collected in `lambdas`) or nested `def`s (their own scopes).
#[derive(Default)]
struct Level<'a> {
    /// `let`s and loop variables: always local to the scope.
    decls: HashSet<String>,
    /// Plain assignment targets: local unless an enclosing scope has the name
    /// (then the assignment writes the enclosing variable).
    assigns: HashSet<String>,
    /// Names read or called.
    refs: HashSet<String>,
    lambdas: Vec<&'a serde_json::Value>,
}

fn scan_level<'a>(v: &'a serde_json::Value, level: &mut Level<'a>) {
    use serde_json::Value as J;
    match v {
        J::Array(items) => items.iter().for_each(|i| scan_level(i, level)),
        J::Object(o) => {
            let field = |k: &str| o.get(k).and_then(J::as_str);
            match field("type") {
                Some("Lambda") => {
                    level.lambdas.push(v);
                    return;
                }
                Some("FunctionDef") => return,
                Some("VarDecl") => {
                    level.decls.extend(field("name").map(str::to_string));
                }
                Some("Assign") => {
                    level.assigns.extend(field("target").map(str::to_string));
                }
                Some("Var") => {
                    level.refs.extend(field("name").map(str::to_string));
                }
                Some("Spawn") => {
                    level.refs.extend(field("function").map(str::to_string));
                }
                // The parser emits `obj.method(..)` as a CapabilityCall named
                // "obj.method"; it loads `obj` when `obj` is a variable.
                Some("CapabilityCall") => {
                    if let Some((receiver, _)) = field("name").and_then(|n| n.split_once('.')) {
                        level.refs.insert(receiver.to_string());
                    }
                }
                Some("Call") => {
                    if let Some(f) = field("function") {
                        if f == "__crush_assign__" {
                            let target = o
                                .get("args")
                                .and_then(|a| a.get(0))
                                .and_then(|t| t.get("name"))
                                .and_then(J::as_str);
                            level.assigns.extend(target.map(str::to_string));
                        } else {
                            // `obj.method(..)` loads `obj`.
                            level.refs.insert(f.split('.').next().unwrap_or(f).to_string());
                        }
                    }
                }
                _ => {}
            }
            level.decls.extend(field("variable").map(str::to_string));
            o.values().for_each(|c| scan_level(c, level));
        }
        _ => {}
    }
}

fn lambda_params(lambda: &serde_json::Value) -> Vec<String> {
    lambda
        .get("params")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| p.get(0).and_then(|n| n.as_str()).map(str::to_string))
        .collect()
}

/// Python scoping (CRUSH-251 step 4): the Python walker marks a lambda it
/// lowered (a `lambda` or a nested `def`) with meta `scope: "python"` and its
/// `nonlocal` names. There, assigning a name makes it local unless it is
/// declared `nonlocal` — unlike Crush / JS, where assigning a name an
/// enclosing scope has writes that variable.
fn python_nonlocals(meta: Option<&serde_json::Value>) -> Option<HashSet<String>> {
    let meta = meta?;
    if meta.get("scope").and_then(|s| s.as_str()) != Some("python") {
        return None;
    }
    Some(
        meta.get("nonlocal")
            .and_then(|n| n.as_array())
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_str().map(str::to_string))
            .collect(),
    )
}

/// The names a lambda's scope binds: parameters, `let`s / loop variables, and
/// assignment targets — those no enclosing scope has, or (Python) those not
/// declared `nonlocal`.
fn lambda_locals(
    params: &[String],
    level: &Level,
    enclosing: &HashSet<String>,
    python_nonlocals: Option<&HashSet<String>>,
) -> HashSet<String> {
    let local_assign = |a: &&String| match python_nonlocals {
        Some(nonlocals) => !nonlocals.contains(*a),
        None => !enclosing.contains(*a),
    };
    // The Python walker lowers every assignment to a `VarDecl`, so a
    // `nonlocal` name must not become local through one either.
    let not_nonlocal = |d: &&String| python_nonlocals.is_none_or(|nl| !nl.contains(*d));
    params
        .iter()
        .chain(level.decls.iter().filter(not_nonlocal))
        .chain(level.assigns.iter().filter(local_assign))
        .cloned()
        .collect()
}

/// CRUSH-251 step 3: the enclosing variables a lambda uses — read, called,
/// assigned, or needed by a nested lambda (transitive capture: a middle lambda
/// captures what an inner one needs so it can pass it down). Sorted: the
/// order of the hidden parameters.
fn lambda_captures(
    params: &[String],
    body: &serde_json::Value,
    own_nonlocals: Option<&HashSet<String>>,
    enclosing: &HashSet<String>,
) -> std::collections::BTreeSet<String> {
    let mut level = Level::default();
    scan_level(body, &mut level);
    let locals = lambda_locals(params, &level, enclosing, own_nonlocals);
    let inner_enclosing: HashSet<String> = enclosing.union(&locals).cloned().collect();
    let mut used: HashSet<String> = level.refs.union(&level.assigns).cloned().collect();
    for nested in &level.lambdas {
        let body = nested.get("body").unwrap_or(&serde_json::Value::Null);
        let nonlocals = python_nonlocals(nested.get("meta"));
        used.extend(lambda_captures(&lambda_params(nested), body, nonlocals.as_ref(), &inner_enclosing));
    }
    used.into_iter()
        .filter(|n| !locals.contains(n) && enclosing.contains(n))
        .collect()
}

/// CRUSH-251 step 3: the variables of a function (and of every lambda inside
/// it) that closures capture by reference, so they live in cells. Capture is
/// by reference — Python and JS closures see later writes and can write the
/// variable — so every captured variable is boxed, except a parameter bound
/// exactly once in the whole function: its value can't change after the
/// closure is made, so the closure gets a copy.
fn boxed_names(params: &[String], body: &serde_json::Value) -> HashSet<String> {
    fn walk(lambdas: &[&serde_json::Value], enclosing: &HashSet<String>, captured: &mut HashSet<String>, all_params: &mut HashSet<String>) {
        for lambda in lambdas {
            let ps = lambda_params(lambda);
            all_params.extend(ps.iter().cloned());
            let body = lambda.get("body").unwrap_or(&serde_json::Value::Null);
            let nonlocals = python_nonlocals(lambda.get("meta"));
            captured.extend(lambda_captures(&ps, body, nonlocals.as_ref(), enclosing));
            let mut level = Level::default();
            scan_level(body, &mut level);
            let inner: HashSet<String> = enclosing
                .union(&lambda_locals(&ps, &level, enclosing, nonlocals.as_ref()))
                .cloned()
                .collect();
            walk(&level.lambdas, &inner, captured, all_params);
        }
    }
    let mut level = Level::default();
    scan_level(body, &mut level);
    let scope: HashSet<String> = params
        .iter()
        .chain(level.decls.iter())
        .chain(level.assigns.iter())
        .cloned()
        .collect();
    let mut captured = HashSet::new();
    let mut all_params: HashSet<String> = params.iter().cloned().collect();
    walk(&level.lambdas, &scope, &mut captured, &mut all_params);
    if captured.is_empty() {
        return captured;
    }
    let cast_params: Vec<(String, CastType)> = params.iter().map(|p| (p.clone(), CastType::Any)).collect();
    let body_stmts: Vec<Statement> = serde_json::from_value(body.clone()).unwrap_or_default();
    let rebound = rebound_names(&cast_params, &body_stmts);
    captured
        .into_iter()
        .filter(|n| !(all_params.contains(n) && !rebound.contains(n)))
        .collect()
}
