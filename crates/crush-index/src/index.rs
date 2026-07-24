//! Core index data structures and ingestion.

use crate::query::{CallSite, CoverageGap};
use crush_cast::manifest::{ExhaustiveMatchSite, FunctionAnnotations, Invariant};
use crush_cast::{Annotation, Expression, Program, Statement};
use std::collections::HashMap;

/// Per-module entry in the index.
#[derive(Debug, Clone)]
pub struct ModuleEntry {
    /// File or module path (from `Program.lang` or a caller-supplied label).
    pub module_path: String,
    /// One-line purpose string from `@module { purpose: "..." }`.
    pub purpose: String,
    /// Exported symbol names.
    pub exports: Vec<String>,
    /// Invariants declared in `@module { invariants: [...] }` or `@invariant` blocks.
    pub invariants: Vec<Invariant>,
    /// Semantically related modules.
    pub related: Vec<String>,
    /// Sum types whose match sites are tracked for exhaustive coverage.
    pub exhaustive_types: Vec<String>,
}

/// Per-function entry in the index.
#[derive(Debug, Clone)]
pub struct FunctionEntry {
    /// Module this function belongs to.
    pub module_path: String,
    /// Function name.
    pub name: String,
    /// Parameter list as `(name, type)` strings.
    pub params: Vec<(String, String)>,
    /// Semantic annotations (`@errors`, `@reads`, etc.), if any were declared.
    pub annotations: Option<FunctionAnnotations>,
    /// Raw function body (as CAST statements, for callers who want to render it).
    pub body_len: usize,
}

/// The cross-reference index for a set of Crush programs.
///
/// Built by calling `index.add_program(module_path, &program)` for each
/// compilation unit.  Queried via the methods on this struct.
pub struct CrushIndex {
    /// module_path → module entry
    modules: HashMap<String, ModuleEntry>,
    /// fn_name → function entry (last write wins when names collide across modules)
    functions: HashMap<String, FunctionEntry>,
    /// fn_name → list of (call_site_module, call_site_fn, call_site_arg_count)
    call_graph: HashMap<String, Vec<CallSite>>,
    /// exhaustive match sites across all programs
    match_sites: Vec<ExhaustiveMatchSite>,
    /// module_path → @wip node (one per module at most)
    wip: HashMap<String, crush_cast::manifest::WipNode>,
    /// (module_path, @temporary node) pairs across all programs
    temporaries: Vec<(String, crush_cast::manifest::TemporaryNode)>,
    /// (module_path, @decision node) pairs across all programs
    decisions: Vec<(String, crush_cast::manifest::DecisionNode)>,
    
    /// CSON configurations indexed by file path (private; see `cson_configs()` and `cson_doc()`)
    cson_configs: HashMap<String, crush_cson::CsonDocument>,
    /// Flattened semantic keys `(intent, cson_file_path, confidence)` (private; see `semantic_keys()`)
    semantic_keys: Vec<(String, String, Option<f64>)>,
    /// Dejavue project timeline events (private; see `dejavue_timeline()`)
    dejavue_timeline: Vec<String>,

    /// CRUSH-28: flat annotation ladders per module, one entry per
    /// `add_program()` call (ladders stack up so cross-module joins
    /// like `@covers` in tests closing `@errors` in impl still work
    /// when a caller adds the same `module_path` more than once).
    /// Private; see [`annotations`](Self::annotations).
    flat_annotations: HashMap<String, Vec<Vec<Annotation>>>,
}

impl CrushIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            functions: HashMap::new(),
            call_graph: HashMap::new(),
            match_sites: Vec::new(),
            wip: HashMap::new(),
            temporaries: Vec::new(),
            decisions: Vec::new(),
            cson_configs: HashMap::new(),
            semantic_keys: Vec::new(),
            dejavue_timeline: Vec::new(),
            flat_annotations: HashMap::new(),
        }
    }

    /// Ingest a compiled program into the index.
    ///
    /// `module_path` is the logical name for this compilation unit (e.g.
    /// `"scheduler"` or `"vm.types"`).  It is used as the module key and
    /// stored in all function entries from this program.
    pub fn add_program(&mut self, module_path: &str, program: &Program) {
        // Module entry from manifest
        let entry = if let Some(manifest) = &program.manifest {
            ModuleEntry {
                module_path: module_path.to_string(),
                purpose: manifest.purpose.clone(),
                exports: manifest.exports.clone(),
                invariants: manifest.invariants.clone(),
                related: manifest.related.clone(),
                exhaustive_types: manifest.exhaustive_types.clone(),
            }
        } else {
            ModuleEntry {
                module_path: module_path.to_string(),
                purpose: String::new(),
                exports: Vec::new(),
                invariants: Vec::new(),
                related: Vec::new(),
                exhaustive_types: Vec::new(),
            }
        };
        self.modules.insert(module_path.to_string(), entry);

        // Function entries + call graph
        for (fn_name, func) in &program.functions {
            self.functions.insert(
                fn_name.clone(),
                FunctionEntry {
                    module_path: module_path.to_string(),
                    name: fn_name.clone(),
                    params: func
                        .params
                        .iter()
                        .map(|(n, t)| (n.clone(), t.to_string()))
                        .collect(),
                    annotations: func.annotations.clone(),
                    body_len: func.body.len(),
                },
            );

            // Walk body to collect outbound calls
            let mut calls: Vec<CallSite> = Vec::new();
            collect_calls_in_stmts(&func.body, module_path, fn_name, &mut calls);
            for call in calls {
                self.call_graph
                    .entry(call.callee.clone())
                    .or_default()
                    .push(call);
            }
        }

        // Exhaustive match sites from the enriched CAST
        self.match_sites.extend(program.exhaustive_sites.clone());

        // @wip and @temporary nodes
        if let Some(wip) = &program.wip {
            self.wip.insert(module_path.to_string(), wip.clone());
        }
        for tmp in &program.temporaries {
            self.temporaries.push((module_path.to_string(), tmp.clone()));
        }
        for dec in &program.decisions {
            self.decisions.push((module_path.to_string(), dec.clone()));
        }

        // CRUSH-28: cache the flat annotation ladder for this module.
        // Append (not overwrite) so two `add_program()` calls with the
        // same `module_path` accumulate ladders — needed for tests that
        // add a "do_thing" program and then a "test_foo" program under
        // the same module so `uncovered_paths()` can detect the gap.
        self.flat_annotations
            .entry(module_path.to_string())
            .or_default()
            .push(program.flatten_annotations());
    }

    // ── query API ────────────────────────────────────────────────────────────

    /// All modules in the index, sorted by module_path.
    ///
    /// Fits in ~20 context lines for a typical workspace — an agent's first
    /// call when starting a session.
    pub fn modules(&self) -> Vec<&ModuleEntry> {
        let mut v: Vec<&ModuleEntry> = self.modules.values().collect();
        v.sort_by(|a, b| a.module_path.cmp(&b.module_path));
        v
    }

    /// Look up a function's signature and contracts by name.
    pub fn definition(&self, fn_name: &str) -> Option<&FunctionEntry> {
        self.functions.get(fn_name)
    }

    /// All call sites that call `fn_name` — i.e., the callers of that function.
    pub fn callers(&self, fn_name: &str) -> Vec<&CallSite> {
        self.call_graph
            .get(fn_name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Invariants declared for a module.
    ///
    /// An agent reads these before touching the module so it knows what must
    /// remain true after the change.
    pub fn invariants(&self, module_path: &str) -> Vec<&Invariant> {
        self.modules
            .get(module_path)
            .map(|m| m.invariants.iter().collect())
            .unwrap_or_default()
    }

    /// All exhaustive match sites for a sum type.
    ///
    /// An agent calls this before adding a new variant to know every match
    /// site that will need a new arm.
    ///
    /// If `type_name` is empty, all sites are returned.
    pub fn exhaustive_sites(&self, type_name: &str) -> Vec<&ExhaustiveMatchSite> {
        self.match_sites
            .iter()
            .filter(|s| type_name.is_empty() || s.type_name == type_name || s.type_name.is_empty())
            .collect()
    }

    /// Error paths (from `@errors`) that have no corresponding `@covers` test.
    ///
    /// Returns one `CoverageGap` per uncovered error variant. An agent
    /// checks this before shipping so it knows which paths are untested.
    ///
    /// CRUSH-28: now consumes the flat annotation ladder instead of
    /// iterating `Function.annotations` directly, so `module_path` is
    /// tracked on each gap and a `@covers` Oracle in module `tests`
    /// correctly closes an `@errors` variant declared in module `impl`.
    pub fn uncovered_paths(&self) -> Vec<CoverageGap> {
        // Errors: from `Annotation::Error` in the flat ladder — preserves
        // module context per-ladder (one ladder per add_program call).
        let mut errors: Vec<(String, String, String)> =
            Vec::new(); // (module_path, fn_name, variant)
        for (mod_path, ladders) in &self.flat_annotations {
            for ladder in ladders {
                for ann in ladder {
                    if let Annotation::Error(e) = ann {
                        for variant in &e.variants {
                            errors.push((
                                mod_path.clone(),
                                e.function_name.clone(),
                                variant.clone(),
                            ));
                        }
                    }
                }
            }
        }

        // Coverage: from `Annotation::Coverage` in the flat ladder. Coverage
        // is module-agnostic (an Oracle name closes a variant regardless
        // of which module declared the @errors), so keep a flat set keyed
        // by variant string.
        let mut covered: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for ladders in self.flat_annotations.values() {
            for ladder in ladders {
                for ann in ladder {
                    if let Annotation::Coverage(c) = ann {
                        for path in &c.paths {
                            covered.insert(path.clone());
                        }
                    }
                }
            }
        }

        errors
            .into_iter()
            .filter(|(_, _, variant)| !covered.contains(variant))
            .map(
                |(module_path, fn_name, error_variant)| CoverageGap {
                    fn_name,
                    error_variant,
                    module_path,
                },
            )
            .collect()
    }

    /// CRUSH-28: flat annotation ladder for a module — the unifying
    /// primitive that downstream `codebase.*` caps (CRUSH-29) and the
    /// dejavue integration (CRUSH-31) iterate over.
    ///
    /// Returns `Annotation::Module`, `Annotation::Invariant`,
    /// function-scoped `Error` / `Read` / `Write` / `Coverage`, and
    /// compiler-populated `ExhaustiveMatchSites` in a single list.
    ///
    /// Multiple `add_program()` calls for the same `module_path` are
    /// flattened across the stacked ladders and sorted by a stable
    /// `(kind, target_resource)` key, so the returned ordering is
    /// reproducible across runs — important for deterministic JSON
    /// export (HashMap iteration order is not).
    ///
    /// **Dedup semantics**: `Annotation::Module` is a singleton per
    /// module_path — re-ingesting the same `module_path` keeps the
    /// first written Module (other Modules are dropped on read); this
    /// is the "first-write-wins" semantic. Other variants stack
    /// freely; an `Annotation::Invariant` dedup by `.name` is a known
    /// gap (filed for the next turn).
    ///
    /// The legacy `Function.annotations` field stays populated for
    /// callers that reach it via `definition(fn_name)`; this method
    /// gives you the LIVING flat view instead.
    pub fn annotations(&self, module_path: &str) -> Vec<&Annotation> {
        match self.flat_annotations.get(module_path) {
            None => Vec::new(),
            Some(ladders) => {
                let mut out: Vec<&Annotation> = ladders.iter().flatten().collect();
                out.sort_by(|a, b| annotation_sort_key(a).cmp(&annotation_sort_key(b)));
                // CRUSH-28 review (Nit Pick Nick): dedup `Annotation::Module`
                // — it's a singleton per module_path. Multiple ladders may
                // carry a Module entry (one per `add_program()` call); only
                // the first survives so downstream `codebase.modules()`
                // (CRUSH-29) doesn't emit duplicate module rows.
                let mut seen_module = false;
                out.retain(|ann| match ann {
                    Annotation::Module(_) if seen_module => false,
                    Annotation::Module(_) => {
                        seen_module = true;
                        true
                    }
                    _ => true,
                });
                out
            }
        }
    }

    /// Number of functions in the index.
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    /// Number of modules in the index.
    pub fn module_count(&self) -> usize {
        self.modules.len()
    }

    /// The @wip node for any module in the index, if one was declared.
    ///
    /// Returns the first wip node found (programs typically have at most one).
    pub fn wip(&self) -> Option<&crush_cast::manifest::WipNode> {
        self.wip.values().next()
    }

    /// All @temporary nodes across all programs.
    pub fn temporaries(&self) -> Vec<&crush_cast::manifest::TemporaryNode> {
        self.temporaries.iter().map(|(_, t)| t).collect()
    }

    /// All @decision nodes across all programs.
    pub fn decisions(&self) -> Vec<&crush_cast::manifest::DecisionNode> {
        self.decisions.iter().map(|(_, d)| d).collect()
    }
    pub fn add_cson(&mut self, path: &str, doc: crush_cson::CsonDocument) {
        // Walk the document root to extract semantic keys
        let mut keys = Vec::new();
        self.extract_semantic_keys(&doc.root, path, &mut keys);
        self.semantic_keys.extend(keys);
        self.cson_configs.insert(path.to_string(), doc);
    }

    fn extract_semantic_keys(&self, node: &crush_cson::CsonNode, path: &str, keys: &mut Vec<(String, String, Option<f64>)>) {
        match &node.value {
            crush_cson::CsonValue::Object(map) => {
                for (k, v) in map {
                    // Object keys are now plain Strings; semantic keys are serialized with "~" prefix
                    if let Some(s) = k.strip_prefix('~') {
                        keys.push((s.to_string(), path.to_string(), v.confidence));
                    }
                    self.extract_semantic_keys(v, path, keys);
                }
            }
            crush_cson::CsonValue::Array(arr) => {
                for v in arr {
                    self.extract_semantic_keys(v, path, keys);
                }
            }
            _ => {}
        }
    }

    /// Load the timeline from `.dejavue/timeline.jsonl` if it exists.
    pub fn load_dejavue(&mut self) {
        if let Ok(content) = std::fs::read_to_string(".dejavue/timeline.jsonl") {
            for line in content.lines() {
                if !line.trim().is_empty() {
                    self.dejavue_timeline.push(line.to_string());
                }
            }
        }
    }

    // ── cson / dejavue accessors ──────────────────────────────────────────────
    //
    // These three fields (`cson_configs`, `semantic_keys`, `dejavue_timeline`)
    // used to be `pub` — an inconsistency vs the rest of the struct's
    // encapsulation. Privatizing and adding these accessor methods keeps
    // the API uniform: every piece of state is reached through a named
    // method whose doc comment names the producer (`add_cson`,
    // `load_dejavue`).

    /// All CSON configurations indexed by file path. Read-only view so
    /// callers cannot bypass the [`add_cson`](Self::add_cson) ingestion
    /// path. For a single document, prefer [`cson_doc`](Self::cson_doc).
    pub fn cson_configs(&self) -> &HashMap<String, crush_cson::CsonDocument> {
        &self.cson_configs
    }

    /// Single CSON document by file path, if any was ingested.
    pub fn cson_doc(&self, path: &str) -> Option<&crush_cson::CsonDocument> {
        self.cson_configs.get(path)
    }

    /// Flattened semantic keys `(intent_key, cson_file_path, confidence)`
    /// extracted from ingested CSON documents.
    pub fn semantic_keys(&self) -> &[(String, String, Option<f64>)] {
        &self.semantic_keys
    }

    /// Dejavue project timeline events loaded by
    /// [`load_dejavue`](Self::load_dejavue). Each entry is one non-empty
    /// line of the `.dejavue/timeline.jsonl` NDJSON stream.
    pub fn dejavue_timeline(&self) -> &[String] {
        &self.dejavue_timeline
    }
}

impl Default for CrushIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ── annotation sort helper ───────────────────────────────────────────────────

/// Stable sort key for [`Annotation`]. Used by
/// [`CrushIndex::annotations`] to produce a deterministic order for JSON
/// export (HashMap iteration order is non-deterministic across runs).
///
/// The first tuple element is the variant ordinal (`Module`=0, `Invariant`=1,
/// `Error`=2, `Read`=3, `Write`=4, `Coverage`=5,
/// `ExhaustiveMatchSites`=6) — tied to the 7-variant enum declared in
/// `crush_cast::manifest::Annotation`. The second element is the
/// `target_resource` key (`function_name` for function-level variants,
/// `invariant.name` for Invariant, `""` for Module) so ties resolve
/// uniformly across runs.
///
/// If `Annotation` gains a variant, extend this match — missing arms
/// will be caught by the compiler.
fn annotation_sort_key(a: &Annotation) -> (u8, String) {
    match a {
        Annotation::Module(_) => (0, String::new()),
        Annotation::Invariant(i) => (1, i.name.clone()),
        Annotation::Error(e) => (2, e.function_name.clone()),
        Annotation::Read(r) => (3, r.function_name.clone()),
        Annotation::Write(w) => (4, w.function_name.clone()),
        Annotation::Coverage(c) => (5, c.function_name.clone()),
        Annotation::ExhaustiveMatchSites(s) => (6, s.function_name.clone()),
    }
}

// ── call-graph walker ─────────────────────────────────────────────────────────

fn collect_calls_in_stmts(
    stmts: &[Statement],
    module: &str,
    caller_fn: &str,
    out: &mut Vec<CallSite>,
) {
    for stmt in stmts {
        collect_calls_in_stmt(stmt, module, caller_fn, out);
    }
}

fn collect_calls_in_stmt(
    stmt: &Statement,
    module: &str,
    caller_fn: &str,
    out: &mut Vec<CallSite>,
) {
    match stmt {
        Statement::ExprStmt { expr, .. } => collect_calls_in_expr(expr, module, caller_fn, out),
        Statement::VarDecl { value, .. } | Statement::Assign { value, .. } | Statement::Export { value, .. } => {
            collect_calls_in_expr(value, module, caller_fn, out)
        }
        Statement::Return { value, .. } => {
            if let Some(v) = value {
                collect_calls_in_expr(v, module, caller_fn, out);
            }
        }
        Statement::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            collect_calls_in_expr(condition, module, caller_fn, out);
            collect_calls_in_stmts(then_body, module, caller_fn, out);
            if let Some(eb) = else_body {
                collect_calls_in_stmts(eb, module, caller_fn, out);
            }
        }
        Statement::While { condition, body, .. } => {
            collect_calls_in_expr(condition, module, caller_fn, out);
            collect_calls_in_stmts(body, module, caller_fn, out);
        }
        Statement::For { iterable, body, .. } => {
            collect_calls_in_expr(iterable, module, caller_fn, out);
            collect_calls_in_stmts(body, module, caller_fn, out);
        }
        Statement::TryCatch { body, handler, .. } => {
            collect_calls_in_stmts(body, module, caller_fn, out);
            collect_calls_in_stmts(handler, module, caller_fn, out);
        }
        Statement::Throw { value, .. } => collect_calls_in_expr(value, module, caller_fn, out),
        Statement::FunctionDef { body, .. } => {
            collect_calls_in_stmts(body, module, caller_fn, out)
        }
        Statement::SetField { target, value, .. } => {
            collect_calls_in_expr(target, module, caller_fn, out);
            collect_calls_in_expr(value, module, caller_fn, out);
        }
        Statement::DomMutate {
            target,
            value,
            value2,
            ..
        } => {
            collect_calls_in_expr(target, module, caller_fn, out);
            if let Some(v) = value {
                collect_calls_in_expr(v, module, caller_fn, out);
            }
            if let Some(v) = value2 {
                collect_calls_in_expr(v, module, caller_fn, out);
            }
        }
        Statement::DomEventListener { target, callback, .. } => {
            collect_calls_in_expr(target, module, caller_fn, out);
            collect_calls_in_expr(callback, module, caller_fn, out);
        }
        Statement::LangBlock { .. }
        | Statement::Import { .. }
        | Statement::StructDef { .. }
        | Statement::Break { .. }
        | Statement::Continue { .. }
        | Statement::AI(_) => {}
    }
}

fn collect_calls_in_expr(
    expr: &Expression,
    module: &str,
    caller_fn: &str,
    out: &mut Vec<CallSite>,
) {
    match expr {
        Expression::Call { function, args, .. } => {
            out.push(CallSite {
                callee: function.clone(),
                caller_module: module.to_string(),
                caller_fn: caller_fn.to_string(),
                arg_count: args.len(),
            });
            for a in args {
                collect_calls_in_expr(a, module, caller_fn, out);
            }
        }
        Expression::CapabilityCall { args, .. } | Expression::Spawn { args, .. } => {
            for a in args {
                collect_calls_in_expr(a, module, caller_fn, out);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_calls_in_expr(left, module, caller_fn, out);
            collect_calls_in_expr(right, module, caller_fn, out);
        }
        Expression::UnaryOp { operand, .. } => {
            collect_calls_in_expr(operand, module, caller_fn, out)
        }
        Expression::Pipeline { segments, .. } => {
            for s in segments {
                collect_calls_in_expr(s, module, caller_fn, out);
            }
        }
        Expression::Lambda { body, .. } => {
            collect_calls_in_stmts(body, module, caller_fn, out)
        }
        Expression::GetField { target, .. } => {
            collect_calls_in_expr(target, module, caller_fn, out)
        }
        Expression::Range { start, end, .. } => {
            collect_calls_in_expr(start, module, caller_fn, out);
            collect_calls_in_expr(end, module, caller_fn, out);
        }
        Expression::Await { expression, .. } => {
            collect_calls_in_expr(expression, module, caller_fn, out)
        }
        Expression::ArrayLiteral { elements, .. }
        | Expression::TupleLiteral { elements, .. }
        | Expression::ListLiteral { elements, .. }
        | Expression::VectorLiteral { elements, .. }
        | Expression::SetLiteral { elements, .. } => {
            for e in elements {
                collect_calls_in_expr(e, module, caller_fn, out);
            }
        }
        Expression::ObjectLiteral { properties, .. } => {
            for (_, v) in properties {
                collect_calls_in_expr(v, module, caller_fn, out);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_calls_in_expr(target, module, caller_fn, out);
            collect_calls_in_expr(index, module, caller_fn, out);
        }
        Expression::DomQuery { selector, .. } => {
            collect_calls_in_expr(selector, module, caller_fn, out)
        }
        Expression::Match {
            expression, arms, ..
        } => {
            collect_calls_in_expr(expression, module, caller_fn, out);
            for arm in arms {
                collect_calls_in_stmts(&arm.body, module, caller_fn, out);
            }
        }
        Expression::IntLiteral { .. }
        | Expression::FloatLiteral { .. }
        | Expression::StringLiteral { .. }
        | Expression::BoolLiteral { .. }
        | Expression::NullLiteral { .. }
        | Expression::Var { .. }
        | Expression::Yield { .. }
        | Expression::NewStruct { .. }
        | Expression::AI(_)
        | Expression::VectorMath { .. } => {}
    }
}
