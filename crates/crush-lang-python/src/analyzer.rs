//! Feature detection and safety analysis for Python code.

use std::collections::HashSet;

use rustpython_ast as py_ast;
use rustpython_parser::Parse;

/// Result of free-variable analysis on a polyglot `@python { ... }` block.
///
/// Used to marshal variables across the Crush/Python boundary without
/// guessing: names the block *reads* but never binds must come from the
/// enclosing Crush scope (inputs); names the block *assigns* at its own
/// top level are candidate outputs to marshal back.
pub struct FreeVars {
    /// Names read (in expression position) but never bound anywhere in
    /// this block, in first-occurrence order. Candidates for input
    /// injection — callers should further filter to names that are
    /// actually declared in the enclosing Crush scope.
    pub reads: Vec<String>,
    /// Names assigned via a plain `name = expr` statement at the block's
    /// own top level (not inside `if`/`for`/`while`/`with`/`try`, and not
    /// inside a nested `def`), in occurrence order. The block's single
    /// output — when the current marshaling protocol needs to pick one —
    /// is conventionally the last entry.
    pub top_level_bound: Vec<String>,
}

/// Analyze a Python source block for free variables, via the real
/// `rustpython-parser` AST — not a regex — so bare identifiers inside
/// strings, f-string literal text, or comments can never be
/// misclassified as variable references.
pub fn free_variables(source: &str) -> Result<FreeVars, String> {
    let suite = py_ast::Suite::parse(source, "<polyglot>").map_err(|e| e.to_string())?;

    // The block runs as module-level code (`python3 -c` with every injected
    // Crush variable bound as a module global), so its own bound set is the
    // module scope. Nested scopes — comprehensions, lambdas, `def`s, `class`
    // bodies — are walked for reads below with their own locals suppressed.
    let module = scope_frame(&suite, &[]);

    let mut reads_seen: Vec<String> = Vec::new();
    let mut reads_dedup: HashSet<String> = HashSet::new();
    let suppress = HashSet::new();
    collect_reads(
        &suite,
        &module,
        &suppress,
        false,
        &mut reads_seen,
        &mut reads_dedup,
    );

    // A `global`-declared read resolves straight to the module scope, so it
    // is emitted above without any nested-scope suppression; this final pass
    // drops the ones that turn out to be bound at module scope after all.
    let reads = reads_seen
        .into_iter()
        .filter(|n| !module.bound.contains(n))
        .collect();

    let mut top_level_bound = Vec::new();
    for stmt in &suite {
        if let py_ast::Stmt::Assign(py_ast::StmtAssign { targets, .. }) = stmt
            && targets.len() == 1
            && let py_ast::Expr::Name(py_ast::ExprName { id, .. }) = &targets[0]
        {
            top_level_bound.push(id.to_string());
        }
    }

    Ok(FreeVars {
        reads,
        top_level_bound,
    })
}

/// The name-resolution state of one Python lexical scope.
///
/// `bound` is the set of names this scope binds *itself* (parameters,
/// assignments, `for`/`with ... as` targets, imports, `def`/`class` names,
/// walrus targets, …). `globals`/`nonlocals` are the `global`/`nonlocal`
/// declarations that redirect resolution past this scope.
struct Frame {
    bound: HashSet<String>,
    globals: HashSet<String>,
    nonlocals: HashSet<String>,
}

impl Frame {
    fn empty() -> Frame {
        Frame {
            bound: HashSet::new(),
            globals: HashSet::new(),
            nonlocals: HashSet::new(),
        }
    }
}

/// Compute the frame for a scope: its own bindings plus its
/// `global`/`nonlocal` declarations. `params` seeds the bound set with a
/// function/lambda's parameter names (the module/class scopes pass none).
fn scope_frame(stmts: &[py_ast::Stmt], params: &[String]) -> Frame {
    let mut frame = Frame {
        bound: params.iter().cloned().collect(),
        globals: HashSet::new(),
        nonlocals: HashSet::new(),
    };
    // `global`/`nonlocal` reach the whole scope even when written inside an
    // `if`/`for`/… body, so collect them first, then bindings.
    collect_scope_decls(stmts, &mut frame);
    collect_bindings(stmts, &mut frame);
    frame
}

/// Collect `global`/`nonlocal` declarations into the frame, recursing into
/// the same-scope compound bodies (which do NOT introduce a new scope) but
/// not into nested `def`/`class` bodies (which do).
fn collect_scope_decls(stmts: &[py_ast::Stmt], frame: &mut Frame) {
    for stmt in stmts {
        match stmt {
            py_ast::Stmt::Global(py_ast::StmtGlobal { names, .. }) => {
                for n in names {
                    frame.globals.insert(n.to_string());
                }
            }
            py_ast::Stmt::Nonlocal(py_ast::StmtNonlocal { names, .. }) => {
                for n in names {
                    frame.nonlocals.insert(n.to_string());
                }
            }
            py_ast::Stmt::If(py_ast::StmtIf { body, orelse, .. }) => {
                collect_scope_decls(body, frame);
                collect_scope_decls(orelse, frame);
            }
            py_ast::Stmt::While(py_ast::StmtWhile { body, orelse, .. }) => {
                collect_scope_decls(body, frame);
                collect_scope_decls(orelse, frame);
            }
            py_ast::Stmt::For(py_ast::StmtFor { body, orelse, .. })
            | py_ast::Stmt::AsyncFor(py_ast::StmtAsyncFor { body, orelse, .. }) => {
                collect_scope_decls(body, frame);
                collect_scope_decls(orelse, frame);
            }
            py_ast::Stmt::With(py_ast::StmtWith { body, .. })
            | py_ast::Stmt::AsyncWith(py_ast::StmtAsyncWith { body, .. }) => {
                collect_scope_decls(body, frame);
            }
            py_ast::Stmt::Try(py_ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            })
            | py_ast::Stmt::TryStar(py_ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                collect_scope_decls(body, frame);
                for py_ast::ExceptHandler::ExceptHandler(h) in handlers {
                    collect_scope_decls(&h.body, frame);
                }
                collect_scope_decls(orelse, frame);
                collect_scope_decls(finalbody, frame);
            }
            py_ast::Stmt::Match(py_ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    collect_scope_decls(&case.body, frame);
                }
            }
            _ => {}
        }
    }
}

/// Collect every name bound in `stmts` at this scope — by assignment,
/// augmented assignment, `for` target, `with ... as`, import, `def`/`class`
/// name, `except ... as`, `match` pattern capture, or walrus `:=` — recursing
/// into control-flow bodies (`if`/`while`/`for`/`with`/`try`/`match`, which do
/// NOT introduce a new Python scope) but NOT into nested `def`/`class`/lambda
/// bodies (which do). Names declared `global`/`nonlocal` are not local
/// bindings here.
fn collect_bindings(stmts: &[py_ast::Stmt], frame: &mut Frame) {
    for stmt in stmts {
        match stmt {
            py_ast::Stmt::Assign(py_ast::StmtAssign { targets, value, .. }) => {
                for t in targets {
                    bind_target(t, frame);
                }
                collect_walrus(value, frame);
            }
            py_ast::Stmt::AugAssign(py_ast::StmtAugAssign { target, value, .. }) => {
                bind_target(target, frame);
                collect_walrus(value, frame);
            }
            py_ast::Stmt::AnnAssign(py_ast::StmtAnnAssign { target, value, .. }) => {
                bind_target(target, frame);
                if let Some(v) = value {
                    collect_walrus(v, frame);
                }
            }
            py_ast::Stmt::TypeAlias(py_ast::StmtTypeAlias { name, value, .. }) => {
                bind_target(name, frame);
                collect_walrus(value, frame);
            }
            py_ast::Stmt::For(py_ast::StmtFor {
                target,
                iter,
                body,
                orelse,
                ..
            })
            | py_ast::Stmt::AsyncFor(py_ast::StmtAsyncFor {
                target,
                iter,
                body,
                orelse,
                ..
            }) => {
                bind_target(target, frame);
                collect_walrus(iter, frame);
                collect_bindings(body, frame);
                collect_bindings(orelse, frame);
            }
            py_ast::Stmt::While(py_ast::StmtWhile {
                test, body, orelse, ..
            }) => {
                collect_walrus(test, frame);
                collect_bindings(body, frame);
                collect_bindings(orelse, frame);
            }
            py_ast::Stmt::If(py_ast::StmtIf {
                test, body, orelse, ..
            }) => {
                collect_walrus(test, frame);
                collect_bindings(body, frame);
                collect_bindings(orelse, frame);
            }
            py_ast::Stmt::With(py_ast::StmtWith { items, body, .. })
            | py_ast::Stmt::AsyncWith(py_ast::StmtAsyncWith { items, body, .. }) => {
                for item in items {
                    collect_walrus(&item.context_expr, frame);
                    if let Some(v) = &item.optional_vars {
                        bind_target(v, frame);
                    }
                }
                collect_bindings(body, frame);
            }
            py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef {
                name,
                args,
                decorator_list,
                returns,
                ..
            })
            | py_ast::Stmt::AsyncFunctionDef(py_ast::StmtAsyncFunctionDef {
                name,
                args,
                decorator_list,
                returns,
                ..
            }) => {
                // The def's own name is bound at this scope; its body is a
                // separate Python scope, so it is deliberately not walked.
                frame.bound.insert(name.to_string());
                for d in decorator_list {
                    collect_walrus(d, frame);
                }
                collect_walrus_args_defaults(args, frame);
                if let Some(r) = returns {
                    collect_walrus(r, frame);
                }
            }
            py_ast::Stmt::ClassDef(py_ast::StmtClassDef {
                name,
                bases,
                keywords,
                decorator_list,
                ..
            }) => {
                frame.bound.insert(name.to_string());
                for d in decorator_list {
                    collect_walrus(d, frame);
                }
                for b in bases {
                    collect_walrus(b, frame);
                }
                for kw in keywords {
                    collect_walrus(&kw.value, frame);
                }
                // body is a separate scope — not walked.
            }
            py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => {
                for alias in names {
                    let bound_name = alias
                        .asname
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            alias
                                .name
                                .to_string()
                                .split('.')
                                .next()
                                .unwrap_or_default()
                                .to_string()
                        });
                    frame.bound.insert(bound_name);
                }
            }
            py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { names, .. }) => {
                for alias in names {
                    let bound_name = alias
                        .asname
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| alias.name.to_string());
                    frame.bound.insert(bound_name);
                }
            }
            py_ast::Stmt::Try(py_ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            })
            | py_ast::Stmt::TryStar(py_ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                collect_bindings(body, frame);
                for py_ast::ExceptHandler::ExceptHandler(h) in handlers {
                    if let Some(name) = &h.name {
                        frame.bound.insert(name.to_string());
                    }
                    if let Some(t) = &h.type_ {
                        collect_walrus(t, frame);
                    }
                    collect_bindings(&h.body, frame);
                }
                collect_bindings(orelse, frame);
                collect_bindings(finalbody, frame);
            }
            py_ast::Stmt::Match(py_ast::StmtMatch { subject, cases, .. }) => {
                collect_walrus(subject, frame);
                for case in cases {
                    bind_pattern(&case.pattern, frame);
                    if let Some(g) = &case.guard {
                        collect_walrus(g, frame);
                    }
                    collect_bindings(&case.body, frame);
                }
            }
            py_ast::Stmt::Expr(py_ast::StmtExpr { value, .. }) => collect_walrus(value, frame),
            py_ast::Stmt::Return(py_ast::StmtReturn { value, .. }) => {
                if let Some(v) = value {
                    collect_walrus(v, frame);
                }
            }
            py_ast::Stmt::Assert(py_ast::StmtAssert { test, msg, .. }) => {
                collect_walrus(test, frame);
                if let Some(m) = msg {
                    collect_walrus(m, frame);
                }
            }
            py_ast::Stmt::Raise(py_ast::StmtRaise { exc, cause, .. }) => {
                if let Some(e) = exc {
                    collect_walrus(e, frame);
                }
                if let Some(c) = cause {
                    collect_walrus(c, frame);
                }
            }
            _ => {}
        }
    }
}

fn bind_target(target: &py_ast::Expr, frame: &mut Frame) {
    match target {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => {
            let name = id.to_string();
            // A name declared `global`/`nonlocal` is not a local binding.
            if !frame.globals.contains(&name) && !frame.nonlocals.contains(&name) {
                frame.bound.insert(name);
            }
        }
        py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. })
        | py_ast::Expr::List(py_ast::ExprList { elts, .. }) => {
            for e in elts {
                bind_target(e, frame);
            }
        }
        py_ast::Expr::Starred(py_ast::ExprStarred { value, .. }) => {
            bind_target(value, frame);
        }
        // Attribute/Subscript targets (`obj.field = x`, `arr[i] = x`) mutate
        // an existing value rather than binding a new name.
        _ => {}
    }
}

/// Bind every name captured by a `match` pattern.
fn bind_pattern(pattern: &py_ast::Pattern, frame: &mut Frame) {
    match pattern {
        py_ast::Pattern::MatchValue(_) | py_ast::Pattern::MatchSingleton(_) => {}
        py_ast::Pattern::MatchSequence(py_ast::PatternMatchSequence { patterns, .. })
        | py_ast::Pattern::MatchOr(py_ast::PatternMatchOr { patterns, .. }) => {
            for p in patterns {
                bind_pattern(p, frame);
            }
        }
        py_ast::Pattern::MatchMapping(py_ast::PatternMatchMapping {
            patterns, rest, ..
        }) => {
            for p in patterns {
                bind_pattern(p, frame);
            }
            if let Some(name) = rest {
                frame.bound.insert(name.to_string());
            }
        }
        py_ast::Pattern::MatchClass(py_ast::PatternMatchClass {
            patterns,
            kwd_patterns,
            ..
        }) => {
            for p in patterns {
                bind_pattern(p, frame);
            }
            for p in kwd_patterns {
                bind_pattern(p, frame);
            }
        }
        py_ast::Pattern::MatchStar(py_ast::PatternMatchStar { name, .. }) => {
            if let Some(name) = name {
                frame.bound.insert(name.to_string());
            }
        }
        py_ast::Pattern::MatchAs(py_ast::PatternMatchAs { pattern, name, .. }) => {
            if let Some(p) = pattern {
                bind_pattern(p, frame);
            }
            if let Some(name) = name {
                frame.bound.insert(name.to_string());
            }
        }
    }
}

/// Collect walrus (`:=`) targets that bind at the current scope. A walrus in
/// a comprehension binds in the comprehension's *enclosing* scope, so
/// comprehensions are transparent here; a lambda body is a new scope, so only
/// its parameter defaults (evaluated in the enclosing scope) are walked.
fn collect_walrus(expr: &py_ast::Expr, frame: &mut Frame) {
    match expr {
        py_ast::Expr::NamedExpr(py_ast::ExprNamedExpr { target, value, .. }) => {
            bind_target(target, frame);
            collect_walrus(value, frame);
        }
        py_ast::Expr::Lambda(py_ast::ExprLambda { args, .. }) => {
            collect_walrus_args_defaults(args, frame);
        }
        py_ast::Expr::ListComp(py_ast::ExprListComp { elt, generators, .. })
        | py_ast::Expr::SetComp(py_ast::ExprSetComp { elt, generators, .. })
        | py_ast::Expr::GeneratorExp(py_ast::ExprGeneratorExp { elt, generators, .. }) => {
            collect_walrus(elt, frame);
            collect_walrus_generators(generators, frame);
        }
        py_ast::Expr::DictComp(py_ast::ExprDictComp {
            key,
            value,
            generators,
            ..
        }) => {
            collect_walrus(key, frame);
            collect_walrus(value, frame);
            collect_walrus_generators(generators, frame);
        }
        py_ast::Expr::BoolOp(py_ast::ExprBoolOp { values, .. }) => {
            for v in values {
                collect_walrus(v, frame);
            }
        }
        py_ast::Expr::BinOp(py_ast::ExprBinOp { left, right, .. }) => {
            collect_walrus(left, frame);
            collect_walrus(right, frame);
        }
        py_ast::Expr::UnaryOp(py_ast::ExprUnaryOp { operand, .. }) => {
            collect_walrus(operand, frame);
        }
        py_ast::Expr::IfExp(py_ast::ExprIfExp {
            test, body, orelse, ..
        }) => {
            collect_walrus(test, frame);
            collect_walrus(body, frame);
            collect_walrus(orelse, frame);
        }
        py_ast::Expr::Dict(py_ast::ExprDict { keys, values, .. }) => {
            for k in keys.iter().flatten() {
                collect_walrus(k, frame);
            }
            for v in values {
                collect_walrus(v, frame);
            }
        }
        py_ast::Expr::Await(py_ast::ExprAwait { value, .. })
        | py_ast::Expr::YieldFrom(py_ast::ExprYieldFrom { value, .. })
        | py_ast::Expr::Attribute(py_ast::ExprAttribute { value, .. })
        | py_ast::Expr::Starred(py_ast::ExprStarred { value, .. }) => {
            collect_walrus(value, frame);
        }
        py_ast::Expr::Yield(py_ast::ExprYield { value, .. }) => {
            if let Some(v) = value {
                collect_walrus(v, frame);
            }
        }
        py_ast::Expr::Compare(py_ast::ExprCompare {
            left, comparators, ..
        }) => {
            collect_walrus(left, frame);
            for c in comparators {
                collect_walrus(c, frame);
            }
        }
        py_ast::Expr::Call(py_ast::ExprCall {
            func,
            args,
            keywords,
            ..
        }) => {
            collect_walrus(func, frame);
            for a in args {
                collect_walrus(a, frame);
            }
            for kw in keywords {
                collect_walrus(&kw.value, frame);
            }
        }
        py_ast::Expr::Subscript(py_ast::ExprSubscript { value, slice, .. }) => {
            collect_walrus(value, frame);
            collect_walrus(slice, frame);
        }
        py_ast::Expr::List(py_ast::ExprList { elts, .. })
        | py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. })
        | py_ast::Expr::Set(py_ast::ExprSet { elts, .. }) => {
            for e in elts {
                collect_walrus(e, frame);
            }
        }
        py_ast::Expr::Slice(py_ast::ExprSlice {
            lower, upper, step, ..
        }) => {
            for e in [lower, upper, step].into_iter().flatten() {
                collect_walrus(e, frame);
            }
        }
        py_ast::Expr::JoinedStr(py_ast::ExprJoinedStr { values, .. }) => {
            for v in values {
                collect_walrus(v, frame);
            }
        }
        py_ast::Expr::FormattedValue(py_ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            collect_walrus(value, frame);
            if let Some(f) = format_spec {
                collect_walrus(f, frame);
            }
        }
        _ => {}
    }
}

fn collect_walrus_generators(generators: &[py_ast::Comprehension], frame: &mut Frame) {
    for g in generators {
        collect_walrus(&g.iter, frame);
        for c in &g.ifs {
            collect_walrus(c, frame);
        }
        // `g.target` is a binding pattern (comprehension-local), never a
        // walrus location.
    }
}

fn collect_walrus_args_defaults(args: &py_ast::Arguments, frame: &mut Frame) {
    for a in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
    {
        if let Some(d) = &a.default {
            collect_walrus(d, frame);
        }
    }
}

/// The suppress set for a nested scope: every name bound by an enclosing
/// scope, so a read of it there does not escape to the block. Class scopes
/// are the exception — Python class bodies do not become the closure scope of
/// functions defined inside them, so a nested `def`/`lambda` must NOT see the
/// class's own bindings (`is_class`).
fn child_suppress(frame: &Frame, suppress: &HashSet<String>, is_class: bool) -> HashSet<String> {
    let mut s = suppress.clone();
    if !is_class {
        s.extend(frame.bound.iter().cloned());
    }
    s
}

/// Collect every `Name` read in expression position, in first-occurrence
/// order, scope-aware. `frame` is the current scope; `suppress` is the union
/// of every enclosing scope's bound set (module scope included). A read is
/// emitted only if it resolves out to the block — i.e. it is not bound in the
/// current scope, not in an enclosing scope, and not `nonlocal`. `global`
/// reads are emitted unconditionally (they resolve to the module scope and are
/// reconciled against the module bound by the caller).
fn collect_reads(
    stmts: &[py_ast::Stmt],
    frame: &Frame,
    suppress: &HashSet<String>,
    is_class: bool,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            py_ast::Stmt::Assign(py_ast::StmtAssign { value, .. }) => {
                read_expr(value, frame, suppress, seen, dedup);
            }
            py_ast::Stmt::AugAssign(py_ast::StmtAugAssign { target, value, .. }) => {
                read_expr(target, frame, suppress, seen, dedup);
                read_expr(value, frame, suppress, seen, dedup);
            }
            py_ast::Stmt::AnnAssign(py_ast::StmtAnnAssign { value, .. }) => {
                if let Some(v) = value {
                    read_expr(v, frame, suppress, seen, dedup);
                }
            }
            py_ast::Stmt::TypeAlias(py_ast::StmtTypeAlias { value, .. }) => {
                read_expr(value, frame, suppress, seen, dedup);
            }
            py_ast::Stmt::Expr(py_ast::StmtExpr { value, .. }) => {
                read_expr(value, frame, suppress, seen, dedup);
            }
            py_ast::Stmt::Return(py_ast::StmtReturn { value, .. }) => {
                if let Some(v) = value {
                    read_expr(v, frame, suppress, seen, dedup);
                }
            }
            py_ast::Stmt::If(py_ast::StmtIf {
                test, body, orelse, ..
            }) => {
                read_expr(test, frame, suppress, seen, dedup);
                collect_reads(body, frame, suppress, is_class, seen, dedup);
                collect_reads(orelse, frame, suppress, is_class, seen, dedup);
            }
            py_ast::Stmt::While(py_ast::StmtWhile {
                test, body, orelse, ..
            }) => {
                read_expr(test, frame, suppress, seen, dedup);
                collect_reads(body, frame, suppress, is_class, seen, dedup);
                collect_reads(orelse, frame, suppress, is_class, seen, dedup);
            }
            py_ast::Stmt::For(py_ast::StmtFor {
                iter, body, orelse, ..
            })
            | py_ast::Stmt::AsyncFor(py_ast::StmtAsyncFor {
                iter, body, orelse, ..
            }) => {
                read_expr(iter, frame, suppress, seen, dedup);
                collect_reads(body, frame, suppress, is_class, seen, dedup);
                collect_reads(orelse, frame, suppress, is_class, seen, dedup);
            }
            py_ast::Stmt::With(py_ast::StmtWith { items, body, .. })
            | py_ast::Stmt::AsyncWith(py_ast::StmtAsyncWith { items, body, .. }) => {
                for item in items {
                    read_expr(&item.context_expr, frame, suppress, seen, dedup);
                }
                collect_reads(body, frame, suppress, is_class, seen, dedup);
            }
            py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef {
                args,
                body,
                decorator_list,
                returns,
                ..
            })
            | py_ast::Stmt::AsyncFunctionDef(py_ast::StmtAsyncFunctionDef {
                args,
                body,
                decorator_list,
                returns,
                ..
            }) => {
                // Decorators, defaults and the return annotation are evaluated
                // in the enclosing scope.
                for d in decorator_list {
                    read_expr(d, frame, suppress, seen, dedup);
                }
                read_defaults(args, frame, suppress, seen, dedup);
                if let Some(r) = returns {
                    read_expr(r, frame, suppress, seen, dedup);
                }
                // The body is a new function scope: its parameters and locals
                // suppress, its free reads bubble up to us.
                let def_frame = scope_frame(body, &args_param_names(args));
                let def_suppress = child_suppress(frame, suppress, is_class);
                collect_reads(body, &def_frame, &def_suppress, false, seen, dedup);
            }
            py_ast::Stmt::ClassDef(py_ast::StmtClassDef {
                bases,
                keywords,
                body,
                decorator_list,
                ..
            }) => {
                for d in decorator_list {
                    read_expr(d, frame, suppress, seen, dedup);
                }
                for b in bases {
                    read_expr(b, frame, suppress, seen, dedup);
                }
                for kw in keywords {
                    read_expr(&kw.value, frame, suppress, seen, dedup);
                }
                // The class body is its own scope, but — unlike a function —
                // it does NOT become the closure scope of nested functions,
                // so `is_class = true` keeps its bindings out of their view.
                let class_frame = scope_frame(body, &[]);
                let class_suppress = child_suppress(frame, suppress, is_class);
                collect_reads(body, &class_frame, &class_suppress, true, seen, dedup);
            }
            py_ast::Stmt::Try(py_ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            })
            | py_ast::Stmt::TryStar(py_ast::StmtTryStar {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                collect_reads(body, frame, suppress, is_class, seen, dedup);
                for py_ast::ExceptHandler::ExceptHandler(h) in handlers {
                    if let Some(t) = &h.type_ {
                        read_expr(t, frame, suppress, seen, dedup);
                    }
                    collect_reads(&h.body, frame, suppress, is_class, seen, dedup);
                }
                collect_reads(orelse, frame, suppress, is_class, seen, dedup);
                collect_reads(finalbody, frame, suppress, is_class, seen, dedup);
            }
            py_ast::Stmt::Assert(py_ast::StmtAssert { test, msg, .. }) => {
                read_expr(test, frame, suppress, seen, dedup);
                if let Some(m) = msg {
                    read_expr(m, frame, suppress, seen, dedup);
                }
            }
            py_ast::Stmt::Raise(py_ast::StmtRaise { exc, cause, .. }) => {
                if let Some(e) = exc {
                    read_expr(e, frame, suppress, seen, dedup);
                }
                if let Some(c) = cause {
                    read_expr(c, frame, suppress, seen, dedup);
                }
            }
            py_ast::Stmt::Match(py_ast::StmtMatch { subject, cases, .. }) => {
                read_expr(subject, frame, suppress, seen, dedup);
                for case in cases {
                    read_pattern(&case.pattern, frame, suppress, seen, dedup);
                    if let Some(g) = &case.guard {
                        read_expr(g, frame, suppress, seen, dedup);
                    }
                    collect_reads(&case.body, frame, suppress, is_class, seen, dedup);
                }
            }
            py_ast::Stmt::Delete(py_ast::StmtDelete { targets, .. }) => {
                for t in targets {
                    read_del_target(t, frame, suppress, seen, dedup);
                }
            }
            _ => {}
        }
    }
}

/// Emit a name read, respecting the current scope and every enclosing scope.
fn emit_read(
    id: &py_ast::Identifier,
    frame: &Frame,
    suppress: &HashSet<String>,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    let name = id.to_string();
    if frame.globals.contains(&name) {
        // `global` resolves straight to the module scope, bypassing every
        // intermediate scope.
        push_read(name, seen, dedup);
    } else if frame.nonlocals.contains(&name)
        || frame.bound.contains(&name)
        || suppress.contains(&name)
    {
        // Bound somewhere inside the block: a comprehension target, a
        // parameter, a nested local, or a `nonlocal` that resolves to an
        // enclosing function — none of which the block must inject.
    } else {
        push_read(name, seen, dedup);
    }
}

fn push_read(name: String, seen: &mut Vec<String>, dedup: &mut HashSet<String>) {
    if dedup.insert(name.clone()) {
        seen.push(name);
    }
}

fn read_expr(
    expr: &py_ast::Expr,
    frame: &Frame,
    suppress: &HashSet<String>,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    match expr {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => {
            emit_read(id, frame, suppress, seen, dedup);
        }
        py_ast::Expr::BoolOp(py_ast::ExprBoolOp { values, .. }) => {
            for v in values {
                read_expr(v, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::NamedExpr(py_ast::ExprNamedExpr { value, .. }) => {
            // The target is a bind, not a read.
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Expr::BinOp(py_ast::ExprBinOp { left, right, .. }) => {
            read_expr(left, frame, suppress, seen, dedup);
            read_expr(right, frame, suppress, seen, dedup);
        }
        py_ast::Expr::UnaryOp(py_ast::ExprUnaryOp { operand, .. }) => {
            read_expr(operand, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Lambda(py_ast::ExprLambda { args, body, .. }) => {
            // Parameter defaults evaluate in the enclosing scope.
            read_defaults(args, frame, suppress, seen, dedup);
            // The body is a new function scope; the parameters suppress.
            let lambda_frame = Frame {
                bound: args_param_names(args).into_iter().collect(),
                globals: HashSet::new(),
                nonlocals: HashSet::new(),
            };
            let lambda_suppress = child_suppress(frame, suppress, false);
            read_expr(body, &lambda_frame, &lambda_suppress, seen, dedup);
        }
        py_ast::Expr::IfExp(py_ast::ExprIfExp {
            test, body, orelse, ..
        }) => {
            read_expr(test, frame, suppress, seen, dedup);
            read_expr(body, frame, suppress, seen, dedup);
            read_expr(orelse, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Dict(py_ast::ExprDict { keys, values, .. }) => {
            for k in keys.iter().flatten() {
                read_expr(k, frame, suppress, seen, dedup);
            }
            for v in values {
                read_expr(v, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::Await(py_ast::ExprAwait { value, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Yield(py_ast::ExprYield { value, .. }) => {
            if let Some(v) = value {
                read_expr(v, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::YieldFrom(py_ast::ExprYieldFrom { value, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Compare(py_ast::ExprCompare {
            left, comparators, ..
        }) => {
            read_expr(left, frame, suppress, seen, dedup);
            for c in comparators {
                read_expr(c, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::Call(py_ast::ExprCall {
            func,
            args,
            keywords,
            ..
        }) => {
            read_expr(func, frame, suppress, seen, dedup);
            for a in args {
                read_expr(a, frame, suppress, seen, dedup);
            }
            for kw in keywords {
                read_expr(&kw.value, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::Attribute(py_ast::ExprAttribute { value, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Subscript(py_ast::ExprSubscript { value, slice, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
            read_expr(slice, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Starred(py_ast::ExprStarred { value, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Expr::List(py_ast::ExprList { elts, .. })
        | py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. })
        | py_ast::Expr::Set(py_ast::ExprSet { elts, .. }) => {
            for e in elts {
                read_expr(e, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::Slice(py_ast::ExprSlice {
            lower, upper, step, ..
        }) => {
            for e in [lower, upper, step].into_iter().flatten() {
                read_expr(e, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::JoinedStr(py_ast::ExprJoinedStr { values, .. }) => {
            for v in values {
                read_expr(v, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::FormattedValue(py_ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            read_expr(value, frame, suppress, seen, dedup);
            if let Some(f) = format_spec {
                read_expr(f, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::ListComp(py_ast::ExprListComp { elt, generators, .. })
        | py_ast::Expr::SetComp(py_ast::ExprSetComp { elt, generators, .. })
        | py_ast::Expr::GeneratorExp(py_ast::ExprGeneratorExp {
            elt, generators, ..
        }) => {
            read_comprehension(elt, None, generators, frame, suppress, seen, dedup);
        }
        py_ast::Expr::DictComp(py_ast::ExprDictComp {
            key,
            value,
            generators,
            ..
        }) => {
            read_comprehension(
                value,
                Some(key),
                generators,
                frame,
                suppress,
                seen,
                dedup,
            );
        }
        // Constant: no reads.
        _ => {}
    }
}

/// Read a comprehension (`elt for … in …`, optionally with a dict `key`).
///
/// The comprehension's own targets are local to it (nested `for`s and `if`s
/// included). Its elt/key/value and inner iterables read against the
/// comprehension scope; its *first* iterable is evaluated in the enclosing
/// scope, per Python's comprehension scoping.
fn read_comprehension(
    elt: &py_ast::Expr,
    key: Option<&py_ast::Expr>,
    generators: &[py_ast::Comprehension],
    frame: &Frame,
    suppress: &HashSet<String>,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    let mut comp_frame = Frame::empty();
    for g in generators {
        bind_target(&g.target, &mut comp_frame);
    }
    let comp_suppress = child_suppress(frame, suppress, false);

    if let Some(k) = key {
        read_expr(k, &comp_frame, &comp_suppress, seen, dedup);
    }
    read_expr(elt, &comp_frame, &comp_suppress, seen, dedup);

    for (idx, g) in generators.iter().enumerate() {
        if idx == 0 {
            // Python evaluates the first iterable in the enclosing scope.
            read_expr(&g.iter, frame, suppress, seen, dedup);
        } else {
            read_expr(&g.iter, &comp_frame, &comp_suppress, seen, dedup);
        }
        for c in &g.ifs {
            read_expr(c, &comp_frame, &comp_suppress, seen, dedup);
        }
    }
}

/// Read the expression parts of a `match` pattern; capture names are binds,
/// not reads.
fn read_pattern(
    pattern: &py_ast::Pattern,
    frame: &Frame,
    suppress: &HashSet<String>,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    match pattern {
        py_ast::Pattern::MatchValue(py_ast::PatternMatchValue { value, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Pattern::MatchSingleton(_) => {}
        py_ast::Pattern::MatchSequence(py_ast::PatternMatchSequence { patterns, .. })
        | py_ast::Pattern::MatchOr(py_ast::PatternMatchOr { patterns, .. }) => {
            for p in patterns {
                read_pattern(p, frame, suppress, seen, dedup);
            }
        }
        py_ast::Pattern::MatchMapping(py_ast::PatternMatchMapping {
            keys, patterns, ..
        }) => {
            for k in keys {
                read_expr(k, frame, suppress, seen, dedup);
            }
            for p in patterns {
                read_pattern(p, frame, suppress, seen, dedup);
            }
        }
        py_ast::Pattern::MatchClass(py_ast::PatternMatchClass {
            cls,
            patterns,
            kwd_patterns,
            ..
        }) => {
            read_expr(cls, frame, suppress, seen, dedup);
            for p in patterns {
                read_pattern(p, frame, suppress, seen, dedup);
            }
            for p in kwd_patterns {
                read_pattern(p, frame, suppress, seen, dedup);
            }
        }
        py_ast::Pattern::MatchStar(_) => {}
        py_ast::Pattern::MatchAs(py_ast::PatternMatchAs { pattern, .. }) => {
            if let Some(p) = pattern {
                read_pattern(p, frame, suppress, seen, dedup);
            }
        }
    }
}

/// Read the parts of a `del` target that are expressions (`del obj.attr` reads
/// `obj`; `del name` is a binding removal, not a read).
fn read_del_target(
    expr: &py_ast::Expr,
    frame: &Frame,
    suppress: &HashSet<String>,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    match expr {
        py_ast::Expr::Name(_) => {}
        py_ast::Expr::Attribute(py_ast::ExprAttribute { value, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Subscript(py_ast::ExprSubscript { value, slice, .. }) => {
            read_expr(value, frame, suppress, seen, dedup);
            read_expr(slice, frame, suppress, seen, dedup);
        }
        py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. })
        | py_ast::Expr::List(py_ast::ExprList { elts, .. }) => {
            for e in elts {
                read_del_target(e, frame, suppress, seen, dedup);
            }
        }
        py_ast::Expr::Starred(py_ast::ExprStarred { value, .. }) => {
            read_del_target(value, frame, suppress, seen, dedup);
        }
        _ => {}
    }
}

fn args_param_names(args: &py_ast::Arguments) -> Vec<String> {
    let mut names = Vec::new();
    for a in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
    {
        names.push(a.def.arg.to_string());
    }
    if let Some(v) = &args.vararg {
        names.push(v.arg.to_string());
    }
    if let Some(k) = &args.kwarg {
        names.push(k.arg.to_string());
    }
    names
}

fn read_defaults(
    args: &py_ast::Arguments,
    frame: &Frame,
    suppress: &HashSet<String>,
    seen: &mut Vec<String>,
    dedup: &mut HashSet<String>,
) {
    for a in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
    {
        if let Some(d) = &a.default {
            read_expr(d, frame, suppress, seen, dedup);
        }
    }
}

/// Result of analyzing Python code for Crush compatibility.
pub struct Analysis {
    pub dangerous_imports: Vec<String>,
    pub unsupported_features: Vec<String>,
    pub can_lower: bool,
}

/// Check if Python code can be lowered to CAST by the Crush Python compiler.
pub fn analyze(source: &str) -> Analysis {
    let mut dangerous_imports = Vec::new();
    let mut unsupported_features = Vec::new();

    let ast = match py_ast::Suite::parse(source, "<crush>") {
        Ok(stmts) => stmts,
        Err(e) => {
            unsupported_features.push(format!("parse error: {}", e));
            return Analysis {
                dangerous_imports,
                unsupported_features,
                can_lower: false,
            };
        }
    };

    for stmt in &ast {
        analyze_stmt(stmt, &mut dangerous_imports, &mut unsupported_features);
    }

    let can_lower = unsupported_features.is_empty();
    Analysis {
        dangerous_imports,
        unsupported_features,
        can_lower,
    }
}

fn analyze_stmt(stmt: &py_ast::Stmt, dangerous: &mut Vec<String>, unsupported: &mut Vec<String>) {
    match stmt {
        py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => {
            for alias in names {
                if is_dangerous_import(&alias.name.to_string()) {
                    dangerous.push(alias.name.to_string());
                }
            }
        }
        py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { module, .. }) => {
            if let Some(module) = module {
                let mod_name = module.to_string();
                if is_dangerous_import(&mod_name) {
                    dangerous.push(mod_name);
                }
            }
        }
        py_ast::Stmt::ClassDef { .. } => unsupported.push("class definitions".to_string()),
        py_ast::Stmt::With { .. } => unsupported.push("with statements".to_string()),
        py_ast::Stmt::Try { .. } | py_ast::Stmt::Raise { .. } => {
            unsupported.push("exception handling".to_string())
        }
        py_ast::Stmt::Match { .. } => unsupported.push("match statements".to_string()),
        py_ast::Stmt::Delete { .. } => unsupported.push("del statements".to_string()),
        py_ast::Stmt::Assert { .. } => unsupported.push("assert statements".to_string()),
        py_ast::Stmt::Global { .. } => unsupported.push("global keyword".to_string()),
        py_ast::Stmt::Nonlocal { .. } => unsupported.push("nonlocal keyword".to_string()),
        _ => {}
    }
}

pub fn is_dangerous_import(module: &str) -> bool {
    let dangerous = [
        "os",
        "sys",
        "subprocess",
        "socket",
        "ctypes",
        "signal",
        "multiprocessing",
        "threading",
        "fcntl",
        "termios",
        "pty",
        "tty",
        "resource",
        "mmap",
        "cffi",
        "importlib",
    ];
    let base = module.split('.').next().unwrap_or(module);
    dangerous.contains(&base)
}

#[cfg(test)]
mod free_var_tests {
    use super::*;

    #[test]
    fn reads_outer_name_and_binds_output() {
        let src = "import math\nresult = math.pow(base, 3)\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["base".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn ignores_identifiers_inside_strings_and_comments() {
        // "base" and "result" appear only as text here — a regex scan
        // would misfire on both; the real parser must not.
        let src = "# uses base and result nowhere real\nx = \"base result\"\n";
        let fv = free_variables(src).expect("parse");
        assert!(fv.reads.is_empty(), "reads should be empty, got {:?}", fv.reads);
        assert_eq!(fv.top_level_bound, vec!["x".to_string()]);
    }

    #[test]
    fn locally_bound_name_is_not_a_free_read() {
        let src = "y = 1\nresult = y * 2\n";
        let fv = free_variables(src).expect("parse");
        assert!(fv.reads.is_empty(), "y is bound locally, not a free read");
        assert_eq!(fv.top_level_bound, vec!["y".to_string(), "result".to_string()]);
    }

    #[test]
    fn last_top_level_assignment_wins_as_the_output_candidate() {
        let src = "a = 1\nb = 2\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.top_level_bound, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(fv.top_level_bound.last(), Some(&"b".to_string()));
    }

    #[test]
    fn assignment_inside_if_is_not_top_level_but_reads_still_free() {
        let src = "if base > 0:\n    inner = base\nresult = base\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["base".to_string()]);
        // Only the unconditional top-level assignment counts as an output
        // candidate; `inner` is assigned inside the `if` body.
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn nested_function_free_reads_are_block_reads() {
        // A name read inside a nested `def`, but not bound there (not a
        // parameter, not a local), is a free read of the block.
        let src = "def helper():\n    return local_only\nresult = helper()\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["local_only".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    // ── issue #225: reads inside comprehension scopes ──────────────────────

    #[test]
    fn issue_225_list_comprehension_reads_injected_variable() {
        // The exact repro: `n` is read only inside the comprehension, so it
        // must be detected as a read; `i` is the comprehension target and
        // must not.
        let src = "import math\nnums = [math.factorial(i) for i in range(1, n + 1)]\nresult = nums\n";
        let fv = free_variables(src).expect("parse");
        // `math` is bound by the block's own `import`, so it is not a free
        // read; `range` is a builtin (filtered later by the caller); `n` is
        // the injected Crush variable the bug failed to detect.
        assert_eq!(fv.reads, vec!["range".to_string(), "n".to_string()]);
        assert_eq!(
            fv.top_level_bound,
            vec!["nums".to_string(), "result".to_string()]
        );
    }

    #[test]
    fn comprehension_target_shadowing_a_crush_var_is_not_read() {
        // `n` is the comprehension target; it shadows any outer `n` and must
        // NOT be injected.
        let src = "nums = [n for n in range(3)]\nresult = nums\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["range".to_string()]);
    }

    #[test]
    fn set_and_dict_comprehensions_read_only_their_iterables() {
        let fv = free_variables("s = {x for x in items}\n").expect("parse");
        assert_eq!(fv.reads, vec!["items".to_string()]);

        let fv = free_variables("d = {k: v for k, v in pairs}\n").expect("parse");
        assert_eq!(fv.reads, vec!["pairs".to_string()]);
    }

    #[test]
    fn generator_expression_reads_outer_names() {
        let fv = free_variables("g = (f(i) for i in seq)\n").expect("parse");
        assert_eq!(fv.reads, vec!["f".to_string(), "seq".to_string()]);
    }

    #[test]
    fn nested_for_and_if_in_comprehension_are_comprehension_scoped() {
        // `j` is a nested target; `pred` is a free read; `n` and `m` are the
        // enclosing-scope iterables.
        let src = "[x + j for x in xs for j in range(n) if pred(x, j, m)]\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(
            fv.reads,
            vec!["xs".to_string(), "range".to_string(), "n".to_string(), "pred".to_string(), "m".to_string()]
        );
        assert!(!fv.reads.contains(&"x".to_string()));
        assert!(!fv.reads.contains(&"j".to_string()));
    }

    // ── lambda / nested def / class ────────────────────────────────────────

    #[test]
    fn lambda_parameters_are_not_reads() {
        let fv = free_variables("f = lambda x: x + n\n").expect("parse");
        assert_eq!(fv.reads, vec!["n".to_string()]);

        // A lambda parameter shadowing a Crush variable must not be injected.
        let fv = free_variables("f = lambda n: n * 2\n").expect("parse");
        assert!(fv.reads.is_empty());
    }

    #[test]
    fn nested_def_reads_crush_var_but_not_its_params_or_locals() {
        let src = "def helper(a):\n    local = 1\n    return a + local + n\nresult = helper(0)\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["n".to_string()]);
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }

    #[test]
    fn class_body_reads_resolve_to_the_block_not_nested_functions() {
        // `n` is read directly in the class body; `other` is read in a method
        // whose enclosing scope is the module (class bodies don't create a
        // closure scope for nested functions), so both are block reads.
        let src = "class C:\n    attr = n\n    def m(self):\n        return other\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["n".to_string(), "other".to_string()]);
        assert!(!fv.reads.contains(&"attr".to_string()));
        assert!(!fv.reads.contains(&"self".to_string()));
    }

    #[test]
    fn global_and_nonlocal_follow_python_semantics() {
        // `global n` makes the nested read resolve to the module scope.
        let src = "def outer():\n    n = 1\n    def inner():\n        global n\n        return n\nresult = outer()\n";
        let fv = free_variables(src).expect("parse");
        assert!(fv.reads.contains(&"n".to_string()), "global n should be a read");

        // `nonlocal n` resolves to the enclosing function's local, not the module.
        let src = "def outer():\n    n = 1\n    def inner():\n        nonlocal n\n        return n\nresult = outer()\n";
        let fv = free_variables(src).expect("parse");
        assert!(!fv.reads.contains(&"n".to_string()), "nonlocal n should not be a read");
    }

    // ── try / except / assert / raise / match / del / with / walrus / f-string

    #[test]
    fn try_except_reads_and_binds_the_exception_name() {
        let src = "try:\n    x = f(n)\nexcept ValueError as e:\n    y = e\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(
            fv.reads,
            vec!["f".to_string(), "n".to_string(), "ValueError".to_string()]
        );
        // `e` is bound by `except ... as e`, not a free read.
        assert!(!fv.reads.contains(&"e".to_string()));
    }

    #[test]
    fn assert_and_raise_read_their_operands() {
        let fv = free_variables("assert n > 0\n").expect("parse");
        assert_eq!(fv.reads, vec!["n".to_string()]);

        let fv = free_variables("raise err\n").expect("parse");
        assert_eq!(fv.reads, vec!["err".to_string()]);
    }

    #[test]
    fn del_reads_subscripts_but_not_names() {
        let fv = free_variables("del arr[0]\n").expect("parse");
        assert_eq!(fv.reads, vec!["arr".to_string()]);

        let fv = free_variables("del x\n").expect("parse");
        assert!(fv.reads.is_empty());
    }

    #[test]
    fn match_pattern_captures_bind_names() {
        let src = "match subject:\n    case (a, b):\n        result = a + b + n\n    case _:\n        pass\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["subject".to_string(), "n".to_string()]);
        assert!(!fv.reads.contains(&"a".to_string()));
        assert!(!fv.reads.contains(&"b".to_string()));
    }

    #[test]
    fn with_as_binds_the_target() {
        let src = "with open(p) as fh:\n    result = fh.read()\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["open".to_string(), "p".to_string()]);
        assert!(!fv.reads.contains(&"fh".to_string()));
    }

    #[test]
    fn walrus_in_comprehension_binds_in_the_enclosing_scope() {
        let src = "[y := i for i in range(n)]\nresult = y\n";
        let fv = free_variables(src).expect("parse");
        assert_eq!(fv.reads, vec!["range".to_string(), "n".to_string()]);
        assert!(!fv.reads.contains(&"y".to_string()), "y is walrus-bound, not a read");
    }

    #[test]
    fn fstring_reads_embedded_names() {
        let fv = free_variables("result = f\"{n} and {m}\"\n").expect("parse");
        assert_eq!(fv.reads, vec!["n".to_string(), "m".to_string()]);
    }
}
