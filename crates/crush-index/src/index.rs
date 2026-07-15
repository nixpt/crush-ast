//! Core index data structures and ingestion.

use crate::query::{CallSite, CoverageGap};
use crush_cast::manifest::{ExhaustiveMatchSite, FunctionAnnotations, Invariant};
use crush_cast::{Expression, Program, Statement};
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
    
    /// CSON configurations indexed by file path
    pub cson_configs: HashMap<String, crush_cson::CsonDocument>,
    /// Flattened semantic keys (intent) -> (cson_file_path, Confidence)
    pub semantic_keys: Vec<(String, String, Option<f64>)>,
    /// Dejavue project timeline events
    pub dejavue_timeline: Vec<String>,
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
    /// Returns one `CoverageGap` per uncovered error variant.  An agent checks
    /// this before shipping so it knows which paths are untested.
    pub fn uncovered_paths(&self) -> Vec<CoverageGap> {
        // Collect all error variants claimed by @errors annotations
        let mut errors: Vec<(String, String)> = Vec::new(); // (fn_name, error_variant)
        for entry in self.functions.values() {
            if let Some(ann) = &entry.annotations {
                for e in &ann.errors {
                    errors.push((entry.name.clone(), e.clone()));
                }
            }
        }

        // Collect all error variants covered by @covers in test functions
        let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();
        for entry in self.functions.values() {
            if let Some(ann) = &entry.annotations {
                for c in &ann.covers {
                    covered.insert(c.clone());
                }
            }
        }

        errors
            .into_iter()
            .filter(|(_, variant)| !covered.contains(variant))
            .map(|(fn_name, error_variant)| CoverageGap {
                fn_name,
                error_variant,
            })
            .collect()
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
                    if let crush_cson::CsonKey::Semantic(s) = k {
                        keys.push((s.clone(), path.to_string(), v.confidence));
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
}

impl Default for CrushIndex {
    fn default() -> Self {
        Self::new()
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
        | Expression::AI(_) => {}
    }
}
