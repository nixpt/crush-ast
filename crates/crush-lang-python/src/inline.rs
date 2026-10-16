//! Inline lowering of a `@python { ... }` block (CRUSH-272).
//!
//! crush-lang-sdk's `PolyglotMode::Lower` compiles a polyglot block's body
//! into the enclosing Crush program instead of emitting `EXEC_LANG` (a
//! `python3` subprocess). The block becomes one function: its inputs (the
//! Crush variables it reads) are the parameters, its body is the block lowered
//! through this walker exactly as a `def` body is, and it returns its declared
//! outputs as an array. The caller (the SDK) supplies the call and the copies
//! across the boundary.
//!
//! Lowering a block as a `def` body rather than as module code keeps it from
//! touching any Crush variable: everything the block binds is a local of its
//! own function, and a `def` inside it is a closure over those locals, which
//! is how module-level code reads its globals.
//!
//! Exact or loud: every construct the walker can't lower is an error naming
//! it, never a placeholder. Statements that are legal in a `def` body but not
//! at module level (`return`, `nonlocal`, `yield`) are refused here, since the
//! block is module-level code to Python and wrapping it in a function would
//! otherwise make them mean something.

use std::collections::HashMap;

use crush_cast::{CastType, Expression, Function, Statement};
use crush_walker_core::LowerCtx;
use rustpython_ast as py_ast;

use crate::lower_expr::{MODULES, with_strict_tuples};
use crate::{lower_stmt, parser};

/// Lower a `@python` block body (dedented) into a function taking `params`
/// and returning `[outputs...]`.
pub fn lower_block_function(
    code: &str,
    params: &[String],
    outputs: &[String],
) -> anyhow::Result<Function> {
    let mut stmts = parser::parse_source(code)?;
    refuse_module_illegal(&stmts)?;
    drop_builtin_module_imports(&mut stmts)?;
    let ctx = LowerCtx::new(code, "<crush>", "python");
    let mut body = with_strict_tuples(|| lower_stmt::lower_def_body(&stmts, &ctx))?;
    let meta: HashMap<String, serde_json::Value> = HashMap::new();
    body.push(Statement::Return {
        value: Some(Expression::ArrayLiteral {
            elements: outputs
                .iter()
                .map(|name| Expression::Var { name: name.clone(), meta: meta.clone() })
                .collect(),
            meta: meta.clone(),
        }),
        meta,
    });
    Ok(Function {
        params: params.iter().map(|p| (p.clone(), CastType::Any)).collect(),
        body,
        ..Default::default()
    })
}

/// `import math` (a module the walker lowers `math.f(...)` calls of into Crush
/// builtins) needs no run-time import: drop it. Any other import has no
/// Crush meaning, so it is refused here rather than failing at run time.
fn drop_builtin_module_imports(stmts: &mut Vec<py_ast::Stmt>) -> anyhow::Result<()> {
    let mut kept = Vec::with_capacity(stmts.len());
    for stmt in stmts.drain(..) {
        if let py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) = &stmt
            && names.iter().all(|a| a.asname.is_none() && MODULES.contains(&a.name.as_str()))
        {
            continue;
        }
        kept.push(stmt);
    }
    *stmts = kept;
    refuse_imports(stmts)
}

fn refuse_imports(stmts: &[py_ast::Stmt]) -> anyhow::Result<()> {
    for stmt in stmts {
        let what = match stmt {
            py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => names
                .iter()
                .map(|a| match &a.asname {
                    Some(alias) => format!("{} as {alias}", a.name),
                    None => a.name.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", "),
            py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { module, .. }) => format!(
                "from {} import ...",
                module.as_ref().map(|m| m.to_string()).unwrap_or_default()
            ),
            _ => {
                for_each_body(stmt, &mut |body| refuse_imports(body))?;
                continue;
            }
        };
        anyhow::bail!(
            "`import {what}`: only a top-level, unaliased `import` of a module the walker \
             maps to Crush builtins ({}) is lowered",
            MODULES.join(", ")
        );
    }
    Ok(())
}

/// Call `f` on each statement list nested directly in `stmt` (not `def` /
/// `class` bodies' own scopes — those are walked too, since an import there
/// is just as unlowerable).
fn for_each_body(
    stmt: &py_ast::Stmt,
    f: &mut dyn FnMut(&[py_ast::Stmt]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    match stmt {
        py_ast::Stmt::FunctionDef(s) => f(&s.body),
        py_ast::Stmt::AsyncFunctionDef(s) => f(&s.body),
        py_ast::Stmt::ClassDef(s) => f(&s.body),
        py_ast::Stmt::If(s) => f(&s.body).and_then(|_| f(&s.orelse)),
        py_ast::Stmt::While(s) => f(&s.body).and_then(|_| f(&s.orelse)),
        py_ast::Stmt::For(s) => f(&s.body).and_then(|_| f(&s.orelse)),
        py_ast::Stmt::With(s) => f(&s.body),
        py_ast::Stmt::Try(s) => {
            f(&s.body)?;
            for py_ast::ExceptHandler::ExceptHandler(h) in &s.handlers {
                f(&h.body)?;
            }
            f(&s.orelse)?;
            f(&s.finalbody)
        }
        py_ast::Stmt::Match(s) => s.cases.iter().try_for_each(|c| f(&c.body)),
        _ => Ok(()),
    }
}

/// `return` / `yield` / `nonlocal` outside any `def`: a `SyntaxError` to
/// CPython, so a block using them must not compile here either.
fn refuse_module_illegal(stmts: &[py_ast::Stmt]) -> anyhow::Result<()> {
    for stmt in stmts {
        match stmt {
            py_ast::Stmt::Return(_) => anyhow::bail!("`return` outside a function (a SyntaxError in Python)"),
            py_ast::Stmt::Nonlocal(_) => {
                anyhow::bail!("`nonlocal` at module level (a SyntaxError in Python)")
            }
            py_ast::Stmt::Expr(py_ast::StmtExpr { value, .. })
                if matches!(value.as_ref(), py_ast::Expr::Yield(_) | py_ast::Expr::YieldFrom(_)) =>
            {
                anyhow::bail!("`yield` outside a function (a SyntaxError in Python)")
            }
            py_ast::Stmt::If(s) => {
                refuse_module_illegal(&s.body)?;
                refuse_module_illegal(&s.orelse)?;
            }
            py_ast::Stmt::While(s) => {
                refuse_module_illegal(&s.body)?;
                refuse_module_illegal(&s.orelse)?;
            }
            py_ast::Stmt::For(s) => {
                refuse_module_illegal(&s.body)?;
                refuse_module_illegal(&s.orelse)?;
            }
            py_ast::Stmt::With(s) => refuse_module_illegal(&s.body)?,
            py_ast::Stmt::Try(s) => {
                refuse_module_illegal(&s.body)?;
                for py_ast::ExceptHandler::ExceptHandler(h) in &s.handlers {
                    refuse_module_illegal(&h.body)?;
                }
                refuse_module_illegal(&s.orelse)?;
                refuse_module_illegal(&s.finalbody)?;
            }
            py_ast::Stmt::Match(s) => {
                for case in &s.cases {
                    refuse_module_illegal(&case.body)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
