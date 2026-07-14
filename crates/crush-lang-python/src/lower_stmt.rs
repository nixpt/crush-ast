//! Python AST statement → CAST statement lowering.

use py_ast::Ranged;
use crush_walker_core::LowerCtx;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crush_cast::{CastType, Expression, ImportStatement, Statement};
use rustpython_ast as py_ast;

use crate::lower_expr::{lower_constant, lower_expr, lower_expr_hoist};

/// Monotonic counter for `try`/`except` synthetic exception-variable names
/// (`__exc_0`, `__exc_1`, ...). Independent of `lower_expr.rs`'s comprehension
/// counter — different prefix, so no collision risk in the flat per-function
/// variable namespace crush-vm uses.
static TRY_TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_exc_temp() -> String {
    let n = TRY_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("__exc_{n}")
}

/// Wrap statements that a comprehension's expression-position lowering
/// hoisted (see `lower_expr_hoist`) around the "real" statement they were
/// hoisted out of. When nothing was hoisted, returns `actual` unchanged —
/// existing non-comprehension code paths are untouched.
///
/// CAST has no block-statement node; `Statement::If(true) { ... }` is an
/// existing primitive that already compiles and runs (see `Statement::If`
/// in `crush-frontend/compiler.rs`) and — because crush-vm has no lexical
/// block scoping — a variable declared inside it is visible afterward just
/// like any other flat-namespace local, which is exactly what letting the
/// hoisted temp array's final assignment show through requires.
fn wrap_hoisted(
    hoisted: Vec<Statement>,
    actual: Statement,
    meta: &HashMap<String, serde_json::Value>,
) -> Statement {
    if hoisted.is_empty() {
        return actual;
    }
    let mut then_body = hoisted;
    then_body.push(actual);
    Statement::If {
        condition: Expression::BoolLiteral {
            value: true,
            meta: meta.clone(),
        },
        then_body,
        else_body: None,
        meta: meta.clone(),
    }
}

/// Lower a Python AST statement to a CAST statement.
pub fn lower_stmt(stmt: &py_ast::Stmt, ctx: &LowerCtx<'_>) -> anyhow::Result<Statement> {
    let offset = u32::from(stmt.start()) as usize;
    let meta = ctx.meta_at(offset);
    match stmt {
        py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef {
            name, args, body, ..
        }) => {
            let params: Vec<(String, CastType)> = args
                .args
                .iter()
                .map(|a| (a.def.arg.to_string(), CastType::Any))
                .collect();
            let mut lowered_body = Vec::new();
            for s in body {
                lowered_body.push(lower_stmt(s, ctx)?);
            }
            Ok(Statement::FunctionDef {
                name: name.to_string(),
                params,
                body: lowered_body,
                meta,
            })
        }
        py_ast::Stmt::Return(py_ast::StmtReturn { value, .. }) => match value {
            Some(v) => {
                let (hoisted, value) = lower_expr_hoist(v, ctx)?;
                let actual = Statement::Return {
                    value: Some(value),
                    meta: meta.clone(),
                };
                Ok(wrap_hoisted(hoisted, actual, &meta))
            }
            None => Ok(Statement::Return { value: None, meta }),
        },
        py_ast::Stmt::Assign(py_ast::StmtAssign { targets, value, .. }) => {
            let (hoisted, value) = lower_expr_hoist(value, ctx)?;
            if targets.len() != 1 {
                anyhow::bail!("multi-target assignment not yet supported");
            }
            let target = &targets[0];
            let actual = match target {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Statement::VarDecl {
                    name: id.to_string(),
                    value,
                    type_hint: CastType::Any,
                    meta: meta.clone(),
                },
                py_ast::Expr::Attribute(py_ast::ExprAttribute {
                    value: obj, attr, ..
                }) => {
                    let target = lower_expr(obj, ctx)?;
                    Statement::SetField {
                        target,
                        field: attr.to_string(),
                        value,
                        meta: meta.clone(),
                    }
                }
                py_ast::Expr::Subscript(py_ast::ExprSubscript {
                    value: obj, slice, ..
                }) => {
                    let target = lower_expr(obj, ctx)?;
                    let index = lower_expr(slice, ctx)?;
                    Statement::ExprStmt {
                        expr: Expression::Call {
                            function: "__crush_setindex__".to_string(),
                            args: vec![target, index, value],
                            meta: meta.clone(),
                        },
                        meta: meta.clone(),
                    }
                }
                _ => anyhow::bail!("unsupported assignment target: {:?}", target),
            };
            Ok(wrap_hoisted(hoisted, actual, &meta))
        }
        py_ast::Stmt::AugAssign(py_ast::StmtAugAssign {
            target, op, value, ..
        }) => {
            let target_name: String = match target.as_ref() {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
                _ => anyhow::bail!("augmented assignment target must be a name"),
            };
            let val = lower_expr(value, ctx)?;
            Ok(Statement::VarDecl {
                name: target_name.to_string(),
                value: Expression::BinaryOp {
                    operator: match op {
                        py_ast::Operator::Add => "+",
                        py_ast::Operator::Sub => "-",
                        py_ast::Operator::Mult => "*",
                        py_ast::Operator::Div => "/",
                        _ => anyhow::bail!("unsupported augmented operator"),
                    }
                    .to_string(),
                    left: Box::new(Expression::Var {
                        name: target_name.to_string(),
                        meta: meta.clone(),
                    }),
                    right: Box::new(val),
                    meta: meta.clone(),
                },
                type_hint: CastType::Any,
                meta,
            })
        }
        py_ast::Stmt::Expr(py_ast::StmtExpr { value, .. }) => {
            let (hoisted, expr) = lower_expr_hoist(value, ctx)?;
            let actual = Statement::ExprStmt {
                expr,
                meta: meta.clone(),
            };
            Ok(wrap_hoisted(hoisted, actual, &meta))
        }
        py_ast::Stmt::If(py_ast::StmtIf {
            test, body, orelse, ..
        }) => {
            let condition = lower_expr(test, ctx)?;
            let then_body: Vec<Statement> = body
                .iter()
                .map(|s| lower_stmt(s, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            let else_body: Option<Vec<Statement>> = if orelse.is_empty() {
                None
            } else {
                Some(
                    orelse
                        .iter()
                        .map(|s| lower_stmt(s, ctx))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };
            Ok(Statement::If {
                condition,
                then_body,
                else_body,
                meta,
            })
        }
        py_ast::Stmt::While(py_ast::StmtWhile { test, body, .. }) => {
            let condition = lower_expr(test, ctx)?;
            let body: Vec<Statement> = body
                .iter()
                .map(|s| lower_stmt(s, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Statement::While {
                condition: Box::new(condition),
                body,
                meta,
            })
        }
        py_ast::Stmt::For(py_ast::StmtFor {
            target, iter, body, ..
        }) => {
            let variable: String = match target.as_ref() {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
                _ => anyhow::bail!("for loop target must be a name"),
            };
            let iterable = lower_expr(iter, ctx)?;
            let body: Vec<Statement> = body
                .iter()
                .map(|s| lower_stmt(s, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Statement::For {
                variable,
                iterable: Box::new(iterable),
                body,
                meta,
            })
        }
        py_ast::Stmt::Break { .. } => Ok(Statement::Break { meta }),
        py_ast::Stmt::Continue { .. } => Ok(Statement::Continue { meta }),
        py_ast::Stmt::Raise(py_ast::StmtRaise { exc, cause, .. }) => {
            if cause.is_some() {
                anyhow::bail!("`raise ... from ...` (exception chaining) not yet supported");
            }
            match exc {
                None => anyhow::bail!(
                    "bare `raise` (re-raising the currently-handled exception) not yet \
                     supported — needs current-exception tracking through nested try blocks"
                ),
                Some(exc_expr) => {
                    let value = lower_raise_value(exc_expr, ctx)?;
                    Ok(Statement::Throw { value, meta })
                }
            }
        }
        py_ast::Stmt::Try(py_ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => lower_try(body, handlers, orelse, finalbody, ctx, meta),
        py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => {
            if names.is_empty() {
                anyhow::bail!("empty import");
            }
            let name = &names[0];
            Ok(Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path: name.name.to_string(),
                    alias: name.asname.as_ref().map(|s| s.to_string()),
                    selective: vec![],
                },
                meta,
            })
        }
        py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { module, names, .. }) => {
            let module_path = module.as_ref().map(|s| s.to_string()).unwrap_or_default();
            let selective: Vec<String> = names.iter().map(|n| n.name.to_string()).collect();
            Ok(Statement::Import {
                import: ImportStatement::CrushModule {
                    module_path,
                    alias: None,
                    selective,
                },
                meta,
            })
        }
        py_ast::Stmt::Pass { .. } => Ok(Statement::ExprStmt {
            expr: Expression::NullLiteral { meta: meta.clone() },
            meta,
        }),
        py_ast::Stmt::Global { .. } => anyhow::bail!("global keyword not yet supported"),
        py_ast::Stmt::Nonlocal { .. } => anyhow::bail!("nonlocal keyword not yet supported"),
        py_ast::Stmt::Delete { .. } => anyhow::bail!("del not yet supported"),
        py_ast::Stmt::Assert { .. } => anyhow::bail!("assert not yet supported"),
        py_ast::Stmt::ClassDef { .. } => anyhow::bail!("class definitions not yet supported"),
        py_ast::Stmt::With { .. } => anyhow::bail!("with statements not yet supported"),
        py_ast::Stmt::Match(py_ast::StmtMatch { subject, cases, .. }) => {
            lower_match(subject, cases, ctx, meta)
        }
        py_ast::Stmt::TypeAlias { .. } => anyhow::bail!("type aliases not yet supported"),
        py_ast::Stmt::AsyncFunctionDef(py_ast::StmtAsyncFunctionDef {
            name, args, body, ..
        }) => {
            let params: Vec<(String, CastType)> = args
                .args
                .iter()
                .map(|a| (a.def.arg.to_string(), CastType::Any))
                .collect();
            let mut lowered_body = Vec::new();
            for s in body {
                lowered_body.push(lower_stmt(s, ctx)?);
            }
            Ok(Statement::FunctionDef {
                name: name.to_string(),
                params,
                body: lowered_body,
                meta,
            })
        }
        py_ast::Stmt::AsyncFor { .. } => anyhow::bail!("async for not yet supported"),
        py_ast::Stmt::AsyncWith { .. } => anyhow::bail!("async with not yet supported"),
        py_ast::Stmt::AnnAssign { .. } => anyhow::bail!("annotated assignment not yet supported"),
        py_ast::Stmt::TryStar { .. } => anyhow::bail!("try* not yet supported"),
    }
}

// ── try / except / finally ──────────────────────────────────────────────────
//
// crush-cast's `Statement::TryCatch { body, error_var, handler, meta }` +
// `Statement::Throw` are already wired to real ENTER_TRY/EXIT_TRY/THROW
// opcodes (crush-vm/src/scheduler.rs's `try_stack`) — this is purely a
// mapping job from Python's richer `Try` AST (multiple typed handlers,
// `as name` binding, `finally`) onto Crush's single body/handler pair.
//
// Design, since `TryCatch` only has ONE handler:
//   - `raise SomeError("msg")` lowers to `Throw(ObjectLiteral{__exc_type__,
//     message, args})` — a self-consistent tag scheme (own choice, not a
//     Python builtin hierarchy: NOT `isinstance`-aware, flat nominal string
//     match only. `except Exception:` will NOT catch a `raise ValueError(...)`
//     under this scheme unless the raiser literally used "Exception" as the
//     tag.  That's a deliberate, documented narrowing, not a hidden bug).
//   - Multiple `except` handlers compile to a single nested if/elif chain
//     inside the one CAST handler, testing `error_var.__exc_type__` against
//     each handler's type name(s); a bare `except:`/`except Exception:` is
//     only accepted as the *last* handler (anything declared after it would
//     be unreachable Python — bailing loud there rather than silently
//     dropping the unreachable handlers).
//   - `except X as name:` binds `name` via a `VarDecl` at the top of that
//     handler's branch, aliasing the shared `error_var`.
//   - `finally` is appended to the end of BOTH the try-body (so it runs on
//     the normal-completion path) AND every terminal branch of the handler
//     chain (each matched handler's body, and the final re-throw fallback)
//     — so it also runs whichever handler runs, and on re-throw when no
//     handler's type matches. Known accepted limitation: an early `return`/
//     `break`/`continue` inside the try body or a handler will skip the
//     appended `finally`, matching the fact that CAST's `TryCatch` has no
//     native finally-block semantics (this is a lowering-level desugar of
//     the common case, not a VM-level guarantee).
//   - `try ... except ... else:` (the rarely-used `orelse` clause) bails
//     loud — no clean desugar attempted for this ticket.

fn lower_raise_value(expr: &py_ast::Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    if let py_ast::Expr::Call(py_ast::ExprCall { func, args, .. }) = expr {
        let type_name = exception_type_name(func)?;
        let offset = u32::from(expr.start()) as usize;
        let meta = ctx.meta_at(offset);
        let lowered_args: Vec<Expression> = args
            .iter()
            .map(|a| lower_expr(a, ctx))
            .collect::<Result<Vec<_>, _>>()?;
        let message = lowered_args
            .first()
            .cloned()
            .unwrap_or_else(|| Expression::NullLiteral { meta: meta.clone() });
        return Ok(Expression::ObjectLiteral {
            properties: vec![
                (
                    "__exc_type__".to_string(),
                    Expression::StringLiteral {
                        value: type_name,
                        meta: meta.clone(),
                    },
                ),
                ("message".to_string(), message),
                (
                    "args".to_string(),
                    Expression::ArrayLiteral {
                        elements: lowered_args,
                        meta: meta.clone(),
                    },
                ),
            ],
            meta,
        });
    }
    // Not a constructor call — re-raising a value already in hand (`raise e`
    // inside an except block) or a bare class reference (`raise ValueError`,
    // no parens). Thrown as-is; a bare class reference loses its would-be
    // tag, which is an accepted narrowing (rare in real code — almost always
    // written with parens).
    lower_expr(expr, ctx)
}

fn exception_type_name(func: &py_ast::Expr) -> anyhow::Result<String> {
    match func {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Ok(id.to_string()),
        py_ast::Expr::Attribute(py_ast::ExprAttribute { attr, .. }) => Ok(attr.to_string()),
        _ => anyhow::bail!("unsupported exception constructor expression: {:?}", func),
    }
}

fn lower_try(
    body: &[py_ast::Stmt],
    handlers: &[py_ast::ExceptHandler],
    orelse: &[py_ast::Stmt],
    finalbody: &[py_ast::Stmt],
    ctx: &LowerCtx<'_>,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Statement> {
    if !orelse.is_empty() {
        anyhow::bail!("`try ... except ... else:` not yet supported");
    }

    let error_var = fresh_exc_temp();

    let finally_stmts: Vec<Statement> = finalbody
        .iter()
        .map(|s| lower_stmt(s, ctx))
        .collect::<Result<Vec<_>, _>>()?;

    let mut try_body: Vec<Statement> = body
        .iter()
        .map(|s| lower_stmt(s, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    try_body.extend(finally_stmts.clone());

    let handler_body = lower_except_chain(handlers, 0, &error_var, &finally_stmts, ctx, &meta)?;

    Ok(Statement::TryCatch {
        body: try_body,
        error_var,
        handler: handler_body,
        meta,
    })
}

/// Build the (possibly nested-if) handler body starting at `handlers[idx]`.
/// `epilogue` (the lowered `finally` statements) is appended at the end of
/// every terminal branch: each matched handler's body, and the final
/// no-handler-matched re-throw.
fn lower_except_chain(
    handlers: &[py_ast::ExceptHandler],
    idx: usize,
    error_var: &str,
    epilogue: &[Statement],
    ctx: &LowerCtx<'_>,
    meta: &HashMap<String, serde_json::Value>,
) -> anyhow::Result<Vec<Statement>> {
    if idx >= handlers.len() {
        // No (more) handlers to try — either there were none at all (a bare
        // `try/finally`) or none of the typed handlers matched: run
        // `finally`, then re-raise so the exception keeps propagating.
        let mut out = epilogue.to_vec();
        out.push(Statement::Throw {
            value: Expression::Var {
                name: error_var.to_string(),
                meta: meta.clone(),
            },
            meta: meta.clone(),
        });
        return Ok(out);
    }

    let py_ast::ExceptHandler::ExceptHandler(py_ast::ExceptHandlerExceptHandler {
        type_,
        name,
        body,
        ..
    }) = &handlers[idx];

    let mut branch_body = Vec::new();
    if let Some(bind_name) = name {
        branch_body.push(Statement::VarDecl {
            name: bind_name.to_string(),
            value: Expression::Var {
                name: error_var.to_string(),
                meta: meta.clone(),
            },
            type_hint: CastType::Any,
            meta: meta.clone(),
        });
    }
    for s in body {
        branch_body.push(lower_stmt(s, ctx)?);
    }
    branch_body.extend(epilogue.iter().cloned());

    match type_ {
        None => {
            // Bare `except:` (or `except Exception:`, treated the same way —
            // this scheme has no isinstance-style hierarchy) — catch-all.
            // Must be last: anything declared after it is unreachable Python;
            // rather than silently dropping those handlers, bail loud.
            if idx + 1 != handlers.len() {
                anyhow::bail!(
                    "a bare `except:` (or untyped catch-all) must be the last handler — \
                     handlers after it would be unreachable"
                );
            }
            Ok(branch_body)
        }
        Some(type_expr) => {
            let type_names = exception_type_names(type_expr)?;
            let cond = build_type_match_cond(&type_names, error_var, meta);
            let rest = lower_except_chain(handlers, idx + 1, error_var, epilogue, ctx, meta)?;
            Ok(vec![Statement::If {
                condition: cond,
                then_body: branch_body,
                else_body: Some(rest),
                meta: meta.clone(),
            }])
        }
    }
}

/// `except Foo:` / `except mod.Foo:` / `except (Foo, Bar):` → the tag
/// name(s) to compare `error_var.__exc_type__` against.
fn exception_type_names(expr: &py_ast::Expr) -> anyhow::Result<Vec<String>> {
    match expr {
        py_ast::Expr::Name(py_ast::ExprName { id, .. }) => Ok(vec![id.to_string()]),
        py_ast::Expr::Attribute(py_ast::ExprAttribute { attr, .. }) => Ok(vec![attr.to_string()]),
        py_ast::Expr::Tuple(py_ast::ExprTuple { elts, .. }) => Ok(elts
            .iter()
            .map(exception_type_names)
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect()),
        _ => anyhow::bail!("unsupported exception-type expression in `except`: {:?}", expr),
    }
}

fn build_type_match_cond(
    type_names: &[String],
    error_var: &str,
    meta: &HashMap<String, serde_json::Value>,
) -> Expression {
    let tag = Expression::GetField {
        target: Box::new(Expression::Var {
            name: error_var.to_string(),
            meta: meta.clone(),
        }),
        field: "__exc_type__".to_string(),
        meta: meta.clone(),
    };
    let mut names = type_names.iter();
    let first = names.next().expect("exception_type_names never returns empty");
    let mut cond = Expression::BinaryOp {
        operator: "==".to_string(),
        left: Box::new(tag.clone()),
        right: Box::new(Expression::StringLiteral {
            value: first.clone(),
            meta: meta.clone(),
        }),
        meta: meta.clone(),
    };
    for name in names {
        cond = Expression::BinaryOp {
            operator: "or".to_string(),
            left: Box::new(cond),
            right: Box::new(Expression::BinaryOp {
                operator: "==".to_string(),
                left: Box::new(tag.clone()),
                right: Box::new(Expression::StringLiteral {
                    value: name.clone(),
                    meta: meta.clone(),
                }),
                meta: meta.clone(),
            }),
            meta: meta.clone(),
        };
    }
    cond
}

#[cfg(test)]
mod try_lowering_tests {
    use super::*;
    use crate::parser::parse_source;

    fn ctx() -> LowerCtx<'static> {
        LowerCtx::new("", "<test>", "python")
    }

    fn lower_one(src: &str) -> Statement {
        let stmts = parse_source(src).expect("parse");
        assert_eq!(stmts.len(), 1, "expected exactly one top-level statement");
        let c = ctx();
        lower_stmt(&stmts[0], &c).expect("lower")
    }

    #[test]
    fn simple_try_except_produces_trycatch() {
        let stmt = lower_one("try:\n    x = 1\nexcept ValueError as e:\n    x = 2\n");
        match stmt {
            Statement::TryCatch { body, error_var, handler, .. } => {
                assert_eq!(body.len(), 1);
                assert!(!error_var.is_empty());
                // A *typed* handler (even the only one) still compiles to a
                // type-check `If` — `except:`/`except Exception:` (untyped)
                // is the only shape that skips the wrapper; see
                // `bare_except_produces_flat_handler_body` below.
                assert_eq!(handler.len(), 1);
                match &handler[0] {
                    Statement::If { then_body, .. } => {
                        // then_body = [VarDecl e = __exc, VarDecl x = 2]
                        assert_eq!(then_body.len(), 2);
                        match &then_body[0] {
                            Statement::VarDecl { name, .. } => assert_eq!(name, "e"),
                            other => panic!("expected `as e` binding VarDecl first, got {other:?}"),
                        }
                    }
                    other => panic!("expected a typed-handler If, got {other:?}"),
                }
            }
            other => panic!("expected Statement::TryCatch, got {other:?}"),
        }
    }

    #[test]
    fn bare_except_produces_flat_handler_body() {
        let stmt = lower_one("try:\n    x = 1\nexcept:\n    x = 2\n");
        match stmt {
            Statement::TryCatch { handler, .. } => {
                // No type to check — no If wrapper, just the handler body.
                assert_eq!(handler.len(), 1);
                match &handler[0] {
                    Statement::VarDecl { name, .. } => assert_eq!(name, "x"),
                    other => panic!("expected flat VarDecl handler body, got {other:?}"),
                }
            }
            other => panic!("expected Statement::TryCatch, got {other:?}"),
        }
    }

    #[test]
    fn multi_handler_try_builds_if_chain_by_exc_type() {
        let stmt = lower_one(
            "try:\n    f()\nexcept ValueError:\n    a = 1\nexcept TypeError:\n    a = 2\n",
        );
        match stmt {
            Statement::TryCatch { handler, .. } => {
                assert_eq!(handler.len(), 1);
                match &handler[0] {
                    Statement::If { condition, else_body, .. } => {
                        // condition should reference __exc_type__
                        let s = format!("{condition:?}");
                        assert!(s.contains("__exc_type__"));
                        assert!(s.contains("ValueError"));
                        assert!(else_body.is_some());
                    }
                    other => panic!("expected an If chain for multiple typed handlers, got {other:?}"),
                }
            }
            other => panic!("expected Statement::TryCatch, got {other:?}"),
        }
    }

    #[test]
    fn bare_except_before_typed_handler_bails() {
        // A bare `except:` that ISN'T last shadows everything after it in
        // real Python (unreachable code) — bail loud rather than silently
        // dropping the unreachable `except ValueError:` handler.
        let stmts = parse_source(
            "try:\n    f()\nexcept:\n    a = 2\nexcept ValueError:\n    a = 1\n",
        )
        .unwrap();
        let c = ctx();
        let err = lower_stmt(&stmts[0], &c).unwrap_err();
        assert!(err.to_string().contains("last handler"));
    }

    #[test]
    fn bare_except_as_last_of_several_handlers_is_fine() {
        // Bare `except:` *is* allowed as the terminal handler.
        let stmt = lower_one(
            "try:\n    f()\nexcept ValueError:\n    a = 1\nexcept:\n    a = 2\n",
        );
        assert!(matches!(stmt, Statement::TryCatch { .. }));
    }

    #[test]
    fn raise_call_lowers_to_throw_of_tagged_object() {
        let stmt = lower_one("raise ValueError(\"bad\")\n");
        match stmt {
            Statement::Throw { value: Expression::ObjectLiteral { properties, .. }, .. } => {
                let tag = properties
                    .iter()
                    .find(|(k, _)| k == "__exc_type__")
                    .expect("should have __exc_type__ property");
                match &tag.1 {
                    Expression::StringLiteral { value, .. } => assert_eq!(value, "ValueError"),
                    other => panic!("expected tag to be a string literal, got {other:?}"),
                }
            }
            other => panic!("expected Statement::Throw(ObjectLiteral), got {other:?}"),
        }
    }

    #[test]
    fn bare_raise_bails_loud() {
        let stmts = parse_source("raise\n").unwrap();
        let c = ctx();
        let err = lower_stmt(&stmts[0], &c).unwrap_err();
        assert!(err.to_string().contains("bare"));
    }
}

// ── match ────────────────────────────────────────────────────────────────
//
// Python's `match` is a *statement*; Crush's `crush_cast::Expression::Match`
// is an *expression*. Wrapped as `Statement::ExprStmt { expr: Match{..} }`
// — the compiler already guarantees every `Match` arm leaves exactly one
// value on the stack (falls back to `push_null` for non-expression arm
// bodies — see `compiler.rs`'s `Expression::Match` compilation), so this
// composes cleanly with the ordinary `ExprStmt` compile-then-pop path with
// no special-casing needed.
//
// `crush_cast::Pattern` only has four shapes (`Literal`, `Identifier`,
// `Struct`, `Wildcard`) versus Python's much richer match-pattern grammar
// (`rustpython_ast::Pattern` has eight variants including sequence/mapping/
// or-patterns and guards) — mapped where there's a clean equivalent, bailing
// loud everywhere else per the ticket's explicit guidance (class patterns,
// guards, and or-patterns are a reasonable line to bail at).

fn lower_match(
    subject: &py_ast::Expr,
    cases: &[py_ast::MatchCase],
    ctx: &LowerCtx<'_>,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Statement> {
    if cases.is_empty() {
        anyhow::bail!("`match` statement with no `case` clauses");
    }
    let expression = Box::new(lower_expr(subject, ctx)?);
    let mut arms = Vec::with_capacity(cases.len());
    for case in cases {
        if case.guard.is_some() {
            anyhow::bail!(
                "`case ... if <guard>:` not yet supported — crush_cast::Pattern has no \
                 guard slot"
            );
        }
        let pattern = lower_match_pattern(&case.pattern)?;
        let mut body: Vec<Statement> = case
            .body
            .iter()
            .map(|s| lower_stmt(s, ctx))
            .collect::<Result<Vec<_>, _>>()?;
        // `crush-frontend/compiler.rs`'s `Expression::Match` compilation
        // treats an arm's *last* statement specially: if it's an `ExprStmt`,
        // it compiles that expression directly (no `pop`) so its value
        // becomes the arm's result — assuming every expression leaves
        // exactly one value on the stack. That assumption doesn't hold for
        // capabilities that return nothing (`print(...)` → `io.print`
        // dispatches to `Ok(None)`, per `crush-vm/src/scheduler.rs`) — a
        // Python match arm ending in a bare `print(...)` (completely
        // ordinary Python) would leave the compiled Match's own stack
        // accounting short by one value; reproduced independently via
        // native `.crush` `match` syntax through `crushc`+`crush-run`,
        // filed via `dejavue plan` as a pre-existing compiler gap, not
        // fixed here (crush-frontend/src/parser/mod.rs's native match-arm
        // parsing is out of this ticket's scope). Rather than touch the
        // shared compiler, always end the lowered body with an explicit
        // known-to-produce-a-value statement, so whatever the Match
        // compiler inspects as "last statement" is always safe — a `return`
        // earlier in the arm still exits the function before this is ever
        // reached, so it's a no-op for arms that already return.
        body.push(Statement::ExprStmt {
            expr: Expression::NullLiteral { meta: meta.clone() },
            meta: meta.clone(),
        });
        arms.push(crush_cast::MatchArm { pattern, body });
    }
    Ok(Statement::ExprStmt {
        expr: Expression::Match {
            expression,
            arms,
            meta: meta.clone(),
        },
        meta,
    })
}

fn lower_match_pattern(pattern: &py_ast::Pattern) -> anyhow::Result<crush_cast::Pattern> {
    match pattern {
        py_ast::Pattern::MatchValue(py_ast::PatternMatchValue { value, .. }) => {
            let expr = lower_match_literal(value)?;
            Ok(crush_cast::Pattern::Literal { value: expr })
        }
        py_ast::Pattern::MatchSingleton(py_ast::PatternMatchSingleton { value, .. }) => {
            let expr = match value {
                py_ast::Constant::None => Expression::NullLiteral {
                    meta: HashMap::new(),
                },
                py_ast::Constant::Bool(b) => Expression::BoolLiteral {
                    value: *b,
                    meta: HashMap::new(),
                },
                _ => anyhow::bail!("unsupported singleton match pattern: {:?}", value),
            };
            Ok(crush_cast::Pattern::Literal { value: expr })
        }
        py_ast::Pattern::MatchAs(py_ast::PatternMatchAs {
            pattern: inner,
            name,
            ..
        }) => match (inner, name) {
            (None, None) => Ok(crush_cast::Pattern::Wildcard),
            (None, Some(id)) => Ok(crush_cast::Pattern::Identifier { name: id.to_string() }),
            (Some(inner_pat), None) => lower_match_pattern(inner_pat),
            (Some(_), Some(_)) => anyhow::bail!(
                "`case <pattern> as <name>:` (binding a name to a sub-pattern match) not \
                 yet supported"
            ),
        },
        py_ast::Pattern::MatchClass(py_ast::PatternMatchClass {
            cls,
            patterns,
            kwd_attrs,
            kwd_patterns,
            ..
        }) => {
            if !patterns.is_empty() {
                anyhow::bail!(
                    "positional class patterns (`case Point(x, y):`) not yet supported — \
                     crush_cast::Pattern::Struct only has named fields; use the keyword \
                     form `case Point(x=x, y=y):`"
                );
            }
            let name = match cls.as_ref() {
                py_ast::Expr::Name(py_ast::ExprName { id, .. }) => id.to_string(),
                py_ast::Expr::Attribute(py_ast::ExprAttribute { attr, .. }) => attr.to_string(),
                _ => anyhow::bail!("unsupported class pattern head: {:?}", cls),
            };
            let mut fields = Vec::with_capacity(kwd_attrs.len());
            for (attr, sub) in kwd_attrs.iter().zip(kwd_patterns.iter()) {
                fields.push((attr.to_string(), lower_match_pattern(sub)?));
            }
            Ok(crush_cast::Pattern::Struct { name, fields })
        }
        py_ast::Pattern::MatchSequence(_) => anyhow::bail!(
            "sequence patterns (`case [a, b]:`) not yet supported — no crush_cast::Pattern \
             equivalent"
        ),
        py_ast::Pattern::MatchMapping(_) => anyhow::bail!(
            "mapping patterns (`case {{'k': v}}:`) not yet supported — no crush_cast::Pattern \
             equivalent"
        ),
        py_ast::Pattern::MatchStar(_) => {
            anyhow::bail!("star patterns (`*rest`) not yet supported")
        }
        py_ast::Pattern::MatchOr(_) => anyhow::bail!(
            "or-patterns (`case 1 | 2:`) not yet supported — no crush_cast::Pattern equivalent"
        ),
    }
}

/// `MatchValue`'s grammar restricts `value` to literals and dotted constant
/// names; we only accept plain literal constants (including negative
/// numbers, which parse as `UnaryOp(USub, Constant(..))`).
fn lower_match_literal(expr: &py_ast::Expr) -> anyhow::Result<Expression> {
    match expr {
        py_ast::Expr::Constant(py_ast::ExprConstant { value, .. }) => {
            lower_constant(value, HashMap::new())
        }
        py_ast::Expr::UnaryOp(py_ast::ExprUnaryOp {
            op: py_ast::UnaryOp::USub,
            operand,
            ..
        }) => match operand.as_ref() {
            py_ast::Expr::Constant(py_ast::ExprConstant {
                value: py_ast::Constant::Int(i),
                ..
            }) => {
                let v: i64 = i
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("integer overflow in match pattern"))?;
                Ok(Expression::IntLiteral {
                    value: -v,
                    meta: HashMap::new(),
                })
            }
            py_ast::Expr::Constant(py_ast::ExprConstant {
                value: py_ast::Constant::Float(f),
                ..
            }) => Ok(Expression::FloatLiteral {
                value: -f,
                meta: HashMap::new(),
            }),
            _ => anyhow::bail!("unsupported negative literal in match pattern: {:?}", operand),
        },
        _ => anyhow::bail!(
            "match-value pattern must be a literal constant, got: {:?}",
            expr
        ),
    }
}

#[cfg(test)]
mod match_lowering_tests {
    use super::*;
    use crate::parser::parse_source;

    fn ctx() -> LowerCtx<'static> {
        LowerCtx::new("", "<test>", "python")
    }

    fn lower_one(src: &str) -> Statement {
        let stmts = parse_source(src).expect("parse");
        assert_eq!(stmts.len(), 1, "expected exactly one top-level statement");
        let c = ctx();
        lower_stmt(&stmts[0], &c).expect("lower")
    }

    #[test]
    fn match_statement_wraps_expression_match_in_exprstmt() {
        let stmt = lower_one(
            "match x:\n    case 1:\n        y = 10\n    case _:\n        y = 0\n",
        );
        match stmt {
            Statement::ExprStmt { expr: Expression::Match { arms, .. }, .. } => {
                assert_eq!(arms.len(), 2);
                match &arms[0].pattern {
                    crush_cast::Pattern::Literal { value: Expression::IntLiteral { value, .. } } => {
                        assert_eq!(*value, 1);
                    }
                    other => panic!("expected literal pattern `1`, got {other:?}"),
                }
                match &arms[1].pattern {
                    crush_cast::Pattern::Wildcard => {}
                    other => panic!("expected wildcard pattern `_`, got {other:?}"),
                }
                // Every arm body must end in a value-producing statement
                // (the NullLiteral safety net) regardless of what the
                // Python source wrote.
                for arm in &arms {
                    assert!(matches!(
                        arm.body.last(),
                        Some(Statement::ExprStmt { expr: Expression::NullLiteral { .. }, .. })
                    ));
                }
            }
            other => panic!("expected ExprStmt(Match), got {other:?}"),
        }
    }

    #[test]
    fn match_capture_pattern_binds_identifier() {
        let stmt = lower_one("match x:\n    case n:\n        y = n\n");
        match stmt {
            Statement::ExprStmt { expr: Expression::Match { arms, .. }, .. } => {
                match &arms[0].pattern {
                    crush_cast::Pattern::Identifier { name } => assert_eq!(name, "n"),
                    other => panic!("expected capture pattern, got {other:?}"),
                }
            }
            other => panic!("expected ExprStmt(Match), got {other:?}"),
        }
    }

    #[test]
    fn match_guard_bails_loud() {
        let stmts = parse_source("match x:\n    case n if n > 0:\n        y = n\n").unwrap();
        let c = ctx();
        let err = lower_stmt(&stmts[0], &c).unwrap_err();
        assert!(err.to_string().contains("guard"));
    }

    #[test]
    fn match_or_pattern_bails_loud() {
        let stmts = parse_source("match x:\n    case 1 | 2:\n        y = 1\n").unwrap();
        let c = ctx();
        let err = lower_stmt(&stmts[0], &c).unwrap_err();
        assert!(err.to_string().contains("or-pattern"));
    }

    #[test]
    fn match_sequence_pattern_bails_loud() {
        let stmts = parse_source("match x:\n    case [a, b]:\n        y = 1\n").unwrap();
        let c = ctx();
        let err = lower_stmt(&stmts[0], &c).unwrap_err();
        assert!(err.to_string().contains("sequence"));
    }

    #[test]
    fn match_class_pattern_keyword_form_lowers_to_struct_pattern() {
        let stmt = lower_one(
            "match p:\n    case Point(x=px, y=py):\n        z = px\n",
        );
        match stmt {
            Statement::ExprStmt { expr: Expression::Match { arms, .. }, .. } => {
                match &arms[0].pattern {
                    crush_cast::Pattern::Struct { name, fields } => {
                        assert_eq!(name, "Point");
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].0, "x");
                        assert_eq!(fields[1].0, "y");
                    }
                    other => panic!("expected Struct pattern, got {other:?}"),
                }
            }
            other => panic!("expected ExprStmt(Match), got {other:?}"),
        }
    }

    #[test]
    fn match_positional_class_pattern_bails_loud() {
        let stmts = parse_source("match p:\n    case Point(px, py):\n        z = 1\n").unwrap();
        let c = ctx();
        let err = lower_stmt(&stmts[0], &c).unwrap_err();
        assert!(err.to_string().contains("positional"));
    }
}
