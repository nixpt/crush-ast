//! syn statement → CAST statement lowering.

use crush_walker_core_no_tree_sitter::LowerCtx;

use crush_cast::{CastType, Expression, Statement};
use quote::ToTokens;
use syn::{Stmt, spanned::Spanned};

use crate::lower_expr::{format_args_to_expr, lower_expr, pat_to_ident};

pub fn lower_stmt(stmt: &Stmt, ctx: &LowerCtx<'_>) -> anyhow::Result<Statement> {
    let start = stmt.span().start();
    lower_stmt_inner(stmt, ctx).map_err(|error| {
        anyhow::anyhow!("Rust statement at {}:{}: {error:#}", start.line, start.column + 1)
    })
}

fn lower_stmt_inner(stmt: &Stmt, ctx: &LowerCtx<'_>) -> anyhow::Result<Statement> {
    let start = stmt.span().start();
    let meta = ctx.meta_lc(start.line, start.column + 1);
    match stmt {
        Stmt::Local(local) => {
            let name = pat_to_ident(&local.pat)?;
            let value = match &local.init {
                Some(init) => lower_expr(&init.expr, ctx)?,
                None => Expression::NullLiteral { meta: meta.clone() },
            };
            Ok(Statement::VarDecl {
                name,
                value,
                type_hint: CastType::Any,
                meta,
            })
        }
        Stmt::Item(item) => match item {
            syn::Item::Fn(item_fn) => {
                let name = item_fn.sig.ident.to_string();
                let params: Vec<(String, CastType)> = item_fn
                    .sig
                    .inputs
                    .iter()
                    .map(|p| match p {
                        syn::FnArg::Typed(pat_type) => {
                            let name = match pat_type.pat.as_ref() {
                                syn::Pat::Ident(pi) => pi.ident.to_string(),
                                _ => "_".to_string(),
                            };
                            (name, CastType::Any)
                        }
                        _ => ("_".to_string(), CastType::Any),
                    })
                    .collect();
                // CRUSH-197: a function with a return type returns its tail
                // expression (`fn f(n: i64) -> i64 { n + 1 }`).
                let tail = if matches!(item_fn.sig.output, syn::ReturnType::Default) { Tail::Discard } else { Tail::Return };
                let body = lower_block(&item_fn.block.stmts, ctx, tail)?;
                Ok(Statement::FunctionDef {
                    name,
                    params,
                    body,
                    meta,
                })
            }
            _ => anyhow::bail!(
                "unsupported Rust item in a block: `{}`",
                item.to_token_stream()
            ),
        },
        Stmt::Expr(expr, _) => match expr {
            syn::Expr::Return(e_ret) => {
                let value = match &e_ret.expr {
                    Some(val_expr) => Some(lower_expr(val_expr, ctx)?),
                    None => None,
                };
                Ok(Statement::Return { value, meta })
            }
            syn::Expr::If(e_if) => {
                let condition = lower_expr(&e_if.cond, ctx)?;
                let mut then_body = Vec::new();
                for s in &e_if.then_branch.stmts {
                    then_body.push(lower_stmt(s, ctx)?);
                }
                let else_body = match &e_if.else_branch {
                    Some((_, else_expr)) => match else_expr.as_ref() {
                        syn::Expr::Block(eb) => {
                            let mut elses = Vec::new();
                            for s in &eb.block.stmts {
                                elses.push(lower_stmt(s, ctx)?);
                            }
                            Some(elses)
                        }
                        syn::Expr::If(nested_if) => {
                            let nested = lower_stmt(&syn::Stmt::Expr(
                                syn::Expr::If(nested_if.clone()),
                                None,
                            ), ctx)?;
                            Some(vec![nested])
                        }
                        _ => {
                            let stmt = lower_stmt(&syn::Stmt::Expr(*else_expr.clone(), None), ctx)?;
                            Some(vec![stmt])
                        }
                    },
                    None => None,
                };
                Ok(Statement::If {
                    condition,
                    then_body,
                    else_body,
                    meta,
                })
            }
            syn::Expr::While(e_while) => {
                let condition = lower_expr(&e_while.cond, ctx)?;
                let mut body = Vec::new();
                for s in &e_while.body.stmts {
                    body.push(lower_stmt(s, ctx)?);
                }
                Ok(Statement::While {
                    condition: Box::new(condition),
                    body,
                    meta,
                })
            }
            syn::Expr::ForLoop(e_for) => {
                let variable = pat_to_ident(&e_for.pat)?;
                let iterable = lower_expr(&e_for.expr, ctx)?;
                let mut body = Vec::new();
                for s in &e_for.body.stmts {
                    body.push(lower_stmt(s, ctx)?);
                }
                Ok(Statement::For {
                    variable,
                    iterable: Box::new(iterable),
                    body,
                    meta,
                })
            }
            syn::Expr::Break(_) => {
                Ok(Statement::Break { meta })
            }
            syn::Expr::Continue(_) => Ok(Statement::Continue { meta }),
            // `loop { ... }` = `while true { ... }` (break/continue work as usual).
            syn::Expr::Loop(e_loop) => Ok(Statement::While {
                condition: Box::new(Expression::BoolLiteral { value: true, meta: meta.clone() }),
                body: lower_block(&e_loop.body.stmts, ctx, Tail::Discard)?,
                meta,
            }),
            syn::Expr::Block(eb) => {
                let body = lower_block(&eb.block.stmts, ctx, Tail::Discard)?;
                Ok(Statement::If {
                    condition: Expression::BoolLiteral { value: true, meta: meta.clone() },
                    then_body: body,
                    else_body: None,
                    meta,
                })
            }
            _ => {
                let expr = lower_expr(expr, ctx)?;
                Ok(Statement::ExprStmt { expr, meta })
            }
        },
        Stmt::Macro(mac) => {
            let mac_name = mac
                .mac
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match mac_name.as_str() {
                // CRUSH-197: the format string and its `{}` arguments are now
                // lowered (they used to be dropped — every program printed blank
                // lines). `print!` still ends the line: there is no newline-free
                // print capability yet.
                "println" | "print" | "eprintln" | "eprint" => Ok(Statement::ExprStmt {
                    expr: Expression::CapabilityCall {
                        name: "io.print".to_string(),
                        args: vec![format_args_to_expr(&mac.mac.tokens, ctx)?],
                        meta,
                    },
                    meta: ctx.meta_at(0),
                }),
                _ => anyhow::bail!("macro invocation not supported: {}!", mac_name),
            }
        }
    }
}

/// What a block's final expression (no trailing `;`) means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tail {
    /// Evaluated for effect (unit functions, loop bodies).
    Discard,
    /// The function's return value.
    Return,
    /// The block's value (match arms): kept as the last expression statement.
    Value,
}

/// Lower a block's statements; the final expression without `;` is handled per `tail`.
pub fn lower_block(stmts: &[Stmt], ctx: &LowerCtx<'_>, tail: Tail) -> anyhow::Result<Vec<Statement>> {
    let mut out = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        let last = i + 1 == stmts.len();
        match (s, last, tail) {
            (Stmt::Expr(e, None), true, Tail::Return) => out.extend(lower_tail_return(e, ctx)?),
            (Stmt::Expr(e, None), true, Tail::Value) if !is_statement_like(e) => {
                let meta = ctx.meta_at(0);
                out.push(Statement::ExprStmt { expr: lower_expr(e, ctx)?, meta });
            }
            _ => out.push(lower_stmt(s, ctx)?),
        }
    }
    Ok(out)
}

fn is_statement_like(e: &syn::Expr) -> bool {
    matches!(
        e,
        syn::Expr::If(_) | syn::Expr::While(_) | syn::Expr::ForLoop(_) | syn::Expr::Loop(_)
            | syn::Expr::Return(_) | syn::Expr::Break(_) | syn::Expr::Continue(_)
            | syn::Expr::Block(_) | syn::Expr::Macro(_) | syn::Expr::Assign(_)
    )
}

/// A tail expression in a function with a return type: branches return their own tails.
pub(crate) fn lower_tail_return(e: &syn::Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<Vec<Statement>> {
    let meta = ctx.meta_at(0);
    Ok(match e {
        syn::Expr::If(e_if) => {
            let then_body = lower_block(&e_if.then_branch.stmts, ctx, Tail::Return)?;
            let else_body = match &e_if.else_branch {
                Some((_, else_expr)) => Some(lower_tail_return(else_expr, ctx)?),
                None => None,
            };
            vec![Statement::If { condition: lower_expr(&e_if.cond, ctx)?, then_body, else_body, meta }]
        }
        syn::Expr::Block(eb) => lower_block(&eb.block.stmts, ctx, Tail::Return)?,
        syn::Expr::Paren(p) => lower_tail_return(&p.expr, ctx)?,
        syn::Expr::While(_) | syn::Expr::ForLoop(_) | syn::Expr::Loop(_) | syn::Expr::Return(_)
        | syn::Expr::Break(_) | syn::Expr::Continue(_) | syn::Expr::Assign(_) => {
            vec![lower_stmt(&Stmt::Expr(e.clone(), None), ctx)?]
        }
        _ => vec![Statement::Return { value: Some(lower_expr(e, ctx)?), meta }],
    })
}
