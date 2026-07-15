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

    let mut bound_anywhere: HashSet<String> = HashSet::new();
    collect_bound(&suite, &mut bound_anywhere);

    let mut reads_seen: Vec<String> = Vec::new();
    let mut reads_dedup: HashSet<String> = HashSet::new();
    collect_reads(&suite, &mut reads_seen, &mut reads_dedup);
    let reads = reads_seen
        .into_iter()
        .filter(|n| !bound_anywhere.contains(n))
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

/// Collect every name bound anywhere in `stmts` — by assignment, augmented
/// assignment, `for` target, `with ... as`, import, or `def`/`class` name —
/// recursing into control-flow bodies (`if`/`while`/`for`/`with`/`try`,
/// which do NOT introduce a new Python scope) but NOT into nested
/// `def`/`class`/lambda bodies (which do). This is the set that
/// distinguishes a genuine free read (comes from outside the block) from
/// a read of something the block defines for itself.
fn collect_bound(stmts: &[py_ast::Stmt], bound: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            py_ast::Stmt::Assign(py_ast::StmtAssign { targets, .. }) => {
                for t in targets {
                    bind_target(t, bound);
                }
            }
            py_ast::Stmt::AugAssign(py_ast::StmtAugAssign { target, .. }) => {
                bind_target(target, bound);
            }
            py_ast::Stmt::AnnAssign(py_ast::StmtAnnAssign { target, .. }) => {
                bind_target(target, bound);
            }
            py_ast::Stmt::For(py_ast::StmtFor { target, body, .. })
            | py_ast::Stmt::AsyncFor(py_ast::StmtAsyncFor { target, body, .. }) => {
                bind_target(target, bound);
                collect_bound(body, bound);
            }
            py_ast::Stmt::If(py_ast::StmtIf { body, orelse, .. }) => {
                collect_bound(body, bound);
                collect_bound(orelse, bound);
            }
            py_ast::Stmt::While(py_ast::StmtWhile { body, orelse, .. }) => {
                collect_bound(body, bound);
                collect_bound(orelse, bound);
            }
            py_ast::Stmt::With(py_ast::StmtWith { items, body, .. })
            | py_ast::Stmt::AsyncWith(py_ast::StmtAsyncWith { items, body, .. }) => {
                for item in items {
                    if let Some(v) = &item.optional_vars {
                        bind_target(v, bound);
                    }
                }
                collect_bound(body, bound);
            }
            py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef { name, .. })
            | py_ast::Stmt::AsyncFunctionDef(py_ast::StmtAsyncFunctionDef { name, .. }) => {
                // The def's own name is bound at this scope; its body is a
                // separate Python scope, so it is deliberately not walked.
                bound.insert(name.to_string());
            }
            py_ast::Stmt::ClassDef(py_ast::StmtClassDef { name, .. }) => {
                bound.insert(name.to_string());
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
                    bound.insert(bound_name);
                }
            }
            py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { names, .. }) => {
                for alias in names {
                    let bound_name = alias
                        .asname
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| alias.name.to_string());
                    bound.insert(bound_name);
                }
            }
            _ => {}
        }
    }
}

fn bind_target(target: &py_ast::Expr, bound: &mut HashSet<String>) {
    match target {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => {
            bound.insert(id.to_string());
        }
        py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. })
        | py_ast::Expr::List(py_ast::ExprList { elts, .. }) => {
            for e in elts {
                bind_target(e, bound);
            }
        }
        py_ast::Expr::Starred(py_ast::ExprStarred { value, .. }) => {
            bind_target(value, bound);
        }
        // Attribute/Subscript targets (`obj.field = x`, `arr[i] = x`) mutate
        // an existing value rather than binding a new name.
        _ => {}
    }
}

/// Collect every `Name` read in expression position, in first-occurrence
/// order, with the same def/class scope boundary as `collect_bound`.
/// Assignment *targets* are intentionally not walked here (a Name being
/// bound is not a read of it); `AugAssign`'s target is the one exception,
/// since `x += 1` genuinely reads the incoming value of `x`.
fn collect_reads(stmts: &[py_ast::Stmt], seen: &mut Vec<String>, dedup: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            py_ast::Stmt::Assign(py_ast::StmtAssign { value, .. }) => {
                read_expr(value, seen, dedup);
            }
            py_ast::Stmt::AugAssign(py_ast::StmtAugAssign { target, value, .. }) => {
                read_expr(target, seen, dedup);
                read_expr(value, seen, dedup);
            }
            py_ast::Stmt::AnnAssign(py_ast::StmtAnnAssign { value, .. }) => {
                if let Some(v) = value {
                    read_expr(v, seen, dedup);
                }
            }
            py_ast::Stmt::Expr(py_ast::StmtExpr { value, .. }) => {
                read_expr(value, seen, dedup);
            }
            py_ast::Stmt::Return(py_ast::StmtReturn { value, .. }) => {
                if let Some(v) = value {
                    read_expr(v, seen, dedup);
                }
            }
            py_ast::Stmt::If(py_ast::StmtIf {
                test, body, orelse, ..
            }) => {
                read_expr(test, seen, dedup);
                collect_reads(body, seen, dedup);
                collect_reads(orelse, seen, dedup);
            }
            py_ast::Stmt::While(py_ast::StmtWhile {
                test, body, orelse, ..
            }) => {
                read_expr(test, seen, dedup);
                collect_reads(body, seen, dedup);
                collect_reads(orelse, seen, dedup);
            }
            py_ast::Stmt::For(py_ast::StmtFor { iter, body, .. })
            | py_ast::Stmt::AsyncFor(py_ast::StmtAsyncFor { iter, body, .. }) => {
                read_expr(iter, seen, dedup);
                collect_reads(body, seen, dedup);
            }
            py_ast::Stmt::With(py_ast::StmtWith { items, body, .. })
            | py_ast::Stmt::AsyncWith(py_ast::StmtAsyncWith { items, body, .. }) => {
                for item in items {
                    read_expr(&item.context_expr, seen, dedup);
                }
                collect_reads(body, seen, dedup);
            }
            // def/class bodies are a separate scope — not walked for reads.
            _ => {}
        }
    }
}

fn read_expr(expr: &py_ast::Expr, seen: &mut Vec<String>, dedup: &mut HashSet<String>) {
    match expr {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => {
            let name = id.to_string();
            if dedup.insert(name.clone()) {
                seen.push(name);
            }
        }
        py_ast::Expr::BoolOp(py_ast::ExprBoolOp { values, .. }) => {
            for v in values {
                read_expr(v, seen, dedup);
            }
        }
        py_ast::Expr::NamedExpr(py_ast::ExprNamedExpr { value, .. }) => {
            read_expr(value, seen, dedup);
        }
        py_ast::Expr::BinOp(py_ast::ExprBinOp { left, right, .. }) => {
            read_expr(left, seen, dedup);
            read_expr(right, seen, dedup);
        }
        py_ast::Expr::UnaryOp(py_ast::ExprUnaryOp { operand, .. }) => {
            read_expr(operand, seen, dedup);
        }
        py_ast::Expr::IfExp(py_ast::ExprIfExp {
            test, body, orelse, ..
        }) => {
            read_expr(test, seen, dedup);
            read_expr(body, seen, dedup);
            read_expr(orelse, seen, dedup);
        }
        py_ast::Expr::Dict(py_ast::ExprDict { keys, values, .. }) => {
            for k in keys.iter().flatten() {
                read_expr(k, seen, dedup);
            }
            for v in values {
                read_expr(v, seen, dedup);
            }
        }
        py_ast::Expr::Await(py_ast::ExprAwait { value, .. }) => {
            read_expr(value, seen, dedup);
        }
        py_ast::Expr::Compare(py_ast::ExprCompare {
            left, comparators, ..
        }) => {
            read_expr(left, seen, dedup);
            for c in comparators {
                read_expr(c, seen, dedup);
            }
        }
        py_ast::Expr::Call(py_ast::ExprCall {
            func,
            args,
            keywords,
            ..
        }) => {
            read_expr(func, seen, dedup);
            for a in args {
                read_expr(a, seen, dedup);
            }
            for kw in keywords {
                read_expr(&kw.value, seen, dedup);
            }
        }
        py_ast::Expr::Attribute(py_ast::ExprAttribute { value, .. }) => {
            read_expr(value, seen, dedup);
        }
        py_ast::Expr::Subscript(py_ast::ExprSubscript { value, slice, .. }) => {
            read_expr(value, seen, dedup);
            read_expr(slice, seen, dedup);
        }
        py_ast::Expr::Starred(py_ast::ExprStarred { value, .. }) => {
            read_expr(value, seen, dedup);
        }
        py_ast::Expr::List(py_ast::ExprList { elts, .. })
        | py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. })
        | py_ast::Expr::Set(py_ast::ExprSet { elts, .. }) => {
            for e in elts {
                read_expr(e, seen, dedup);
            }
        }
        py_ast::Expr::Slice(py_ast::ExprSlice {
            lower, upper, step, ..
        }) => {
            for e in [lower, upper, step].into_iter().flatten() {
                read_expr(e, seen, dedup);
            }
        }
        py_ast::Expr::JoinedStr(py_ast::ExprJoinedStr { values, .. }) => {
            for v in values {
                read_expr(v, seen, dedup);
            }
        }
        py_ast::Expr::FormattedValue(py_ast::ExprFormattedValue { value, .. }) => {
            read_expr(value, seen, dedup);
        }
        // Constant/Lambda/comprehensions/Yield etc: no free-variable reads
        // we track for the MVP (comprehensions have their own scope in
        // real Python 3; lambdas are rejected earlier by the lowerer).
        _ => {}
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
    fn nested_function_scope_is_not_walked() {
        let src = "def helper():\n    return local_only\nresult = helper()\n";
        let fv = free_variables(src).expect("parse");
        // `local_only` lives inside helper()'s own scope, never walked.
        assert!(!fv.reads.contains(&"local_only".to_string()));
        assert_eq!(fv.top_level_bound, vec!["result".to_string()]);
    }
}
