//! syn Expression → CAST Expression lowering.

use std::collections::HashMap;

use walker_core::LowerCtx;

use crush_cast::{CastType, Expression, Statement};
use syn::Expr;

pub fn lower_expr(expr: &Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    let meta = ctx.meta_at(0);
    match expr {
        Expr::Lit(e) => lower_lit(&e.lit, meta),
        Expr::Path(e) => {
            let name = e
                .path
                .get_ident()
                .ok_or_else(|| anyhow::anyhow!("complex path not supported"))?
                .to_string();
            Ok(Expression::Var { name, meta })
        }
        Expr::Binary(e) => {
            let left = lower_expr(&e.left, ctx)?;
            let right = lower_expr(&e.right, ctx)?;
            let operator = match e.op {
                syn::BinOp::Add(_) => "+",
                syn::BinOp::Sub(_) => "-",
                syn::BinOp::Mul(_) => "*",
                syn::BinOp::Div(_) => "/",
                syn::BinOp::Rem(_) => "%",
                syn::BinOp::Eq(_) => "==",
                syn::BinOp::Ne(_) => "!=",
                syn::BinOp::Lt(_) => "<",
                syn::BinOp::Le(_) => "<=",
                syn::BinOp::Gt(_) => ">",
                syn::BinOp::Ge(_) => ">=",
                syn::BinOp::And(_) => "&&",
                syn::BinOp::Or(_) => "||",
                syn::BinOp::BitAnd(_) => "&",
                syn::BinOp::BitOr(_) => "|",
                syn::BinOp::BitXor(_) => "^",
                syn::BinOp::Shl(_) => "<<",
                syn::BinOp::Shr(_) => ">>",
                syn::BinOp::AddAssign(_) | syn::BinOp::SubAssign(_) => {
                    let operator = match e.op {
                        syn::BinOp::AddAssign(_) => "+",
                        syn::BinOp::SubAssign(_) => "-",
                        _ => unreachable!(),
                    };
                    return Ok(Expression::Call {
                        function: "__crush_assign__".to_string(),
                        args: vec![
                            left.clone(),
                            Expression::BinaryOp {
                                operator: operator.to_string(),
                                left: Box::new(left),
                                right: Box::new(right),
                                meta: meta.clone(),
                            },
                        ],
                        meta,
                    });
                }
                _ => anyhow::bail!("unsupported binary operator: {:?}", e.op),
            };
            Ok(Expression::BinaryOp {
                operator: operator.to_string(),
                left: Box::new(left),
                right: Box::new(right),
                meta,
            })
        }
        Expr::Unary(e) => {
            let operand = lower_expr(&e.expr, ctx)?;
            let operator = match e.op {
                syn::UnOp::Deref(_) => "*",
                syn::UnOp::Not(_) => "!",
                syn::UnOp::Neg(_) => "-",
                _ => anyhow::bail!("unsupported unary operator: {:?}", e.op),
            };
            Ok(Expression::UnaryOp {
                operator: operator.to_string(),
                operand: Box::new(operand),
                meta,
            })
        }
        Expr::Call(e) => {
            let lowered_args: Vec<Expression> = e
                .args
                .iter()
                .map(|a| lower_expr(a, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            let func_name = match e.func.as_ref() {
                Expr::Path(p) => p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
                _ => anyhow::bail!("complex function expression not supported"),
            };
            match func_name.as_str() {
                "println" | "print" => Ok(Expression::CapabilityCall {
                    name: "io.print".to_string(),
                    args: lowered_args,
                    meta,
                }),
                "len" => Ok(Expression::Call {
                    function: "len".to_string(),
                    args: lowered_args,
                    meta,
                }),
                _ => Ok(Expression::Call {
                    function: func_name,
                    args: lowered_args,
                    meta,
                }),
            }
        }
        Expr::Assign(e) => {
            let left = lower_expr(&e.left, ctx)?;
            let right = lower_expr(&e.right, ctx)?;
            Ok(Expression::Call {
                function: "__crush_assign__".to_string(),
                args: vec![left, right],
                meta,
            })
        }
        Expr::If(e) => {
            let condition = lower_expr(&e.cond, ctx)?;
            // Lower then_branch (a Block) as expression
            let then_body = block_to_expr(&e.then_branch, ctx)?;
            let else_body = match &e.else_branch {
                Some((_, else_expr)) => lower_expr(else_expr, ctx)?,
                None => Expression::NullLiteral {
                    meta: ctx.meta_at(0),
                },
            };
            Ok(Expression::Call {
                function: "__crush_ifexpr__".to_string(),
                args: vec![condition, then_body, else_body],
                meta,
            })
        }
        Expr::Block(e) => {
            if e.block.stmts.is_empty() {
                return Ok(Expression::NullLiteral {
                    meta: ctx.meta_at(0),
                });
            }
            let first = &e.block.stmts[0];
            match first {
                syn::Stmt::Expr(expr, _) => lower_expr(expr, ctx),
                syn::Stmt::Macro(_) => anyhow::bail!("macro in expression not supported"),
                syn::Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        let name = pat_to_ident(&local.pat)?;
                        let value = lower_expr(&init.expr, ctx)?;
                        Ok(Expression::Call {
                            function: "__crush_let__".to_string(),
                            args: vec![
                                Expression::Var {
                                    name,
                                    meta: ctx.meta_at(0),
                                },
                                value,
                            ],
                            meta: ctx.meta_at(0),
                        })
                    } else {
                        Ok(Expression::NullLiteral {
                            meta: ctx.meta_at(0),
                        })
                    }
                }
                syn::Stmt::Item(_) => anyhow::bail!("item in block expression not supported"),
            }
        }
        Expr::Paren(e) => lower_expr(&e.expr, ctx),
        Expr::Return(e) => {
            let value = match &e.expr {
                Some(expr) => lower_expr(expr, ctx)?,
                None => Expression::NullLiteral {
                    meta: ctx.meta_at(0),
                },
            };
            Ok(Expression::Call {
                function: "__crush_return__".to_string(),
                args: vec![value],
                meta,
            })
        }
        Expr::MethodCall(e) => {
            let receiver = lower_expr(&e.receiver, ctx)?;
            let mut args = vec![receiver];
            for arg in &e.args {
                args.push(lower_expr(arg, ctx)?);
            }
            let func_name = e.method.to_string();
            Ok(Expression::Call {
                function: func_name,
                args,
                meta,
            })
        }
        Expr::Index(e) => {
            let collection = lower_expr(&e.expr, ctx)?;
            let index = lower_expr(&e.index, ctx)?;
            Ok(Expression::Index {
                target: Box::new(collection),
                index: Box::new(index),
                meta,
            })
        }
        Expr::Range(e) => {
            let start = match &e.start {
                Some(s) => lower_expr(s, ctx)?,
                None => Expression::IntLiteral { value: 0, meta: ctx.meta_at(0) },
            };
            let end = match &e.end {
                Some(end) => lower_expr(end, ctx)?,
                None => Expression::NullLiteral { meta: ctx.meta_at(0) },
            };
            Ok(Expression::Range {
                start: Box::new(start),
                end: Box::new(end),
                meta,
            })
        }
        Expr::Cast(e) => {
            lower_expr(&e.expr, ctx)
        }
        Expr::Field(e) => {
            let target = lower_expr(&e.base, ctx)?;
            let field = match &e.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(index) => index.index.to_string(),
            };
            Ok(Expression::GetField {
                target: Box::new(target),
                field,
                meta,
            })
        }
        Expr::Array(e) => {
            let elements: Vec<Expression> = e.elems.iter()
                .map(|elem| lower_expr(elem, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        Expr::Closure(e) => {
            let params: Vec<(String, CastType)> = e.inputs.iter()
                .map(|p| {
                    let name = match p {
                        syn::Pat::Ident(pi) => pi.ident.to_string(),
                        _ => "_param".to_string(),
                    };
                    (name, CastType::Any)
                })
                .collect();
            let body_expr = lower_expr(&e.body, ctx)?;
            Ok(Expression::Lambda {
                params,
                body: vec![Statement::Return { value: Some(body_expr), meta: meta.clone() }],
                meta,
            })
        }
        Expr::Reference(e) => {
            lower_expr(&e.expr, ctx)
        }
        _ => {
            eprintln!("unsupported Rust expression: {:#?}", expr);
            anyhow::bail!("unsupported Rust expression")
        },
    }
}

fn block_to_expr(block: &syn::Block, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    if block.stmts.is_empty() {
        return Ok(Expression::NullLiteral {
            meta: ctx.meta_at(0),
        });
    }
    match &block.stmts[0] {
        syn::Stmt::Expr(expr, _) => lower_expr(expr, ctx),
        syn::Stmt::Macro(_) => anyhow::bail!("macro in block not supported"),
        syn::Stmt::Local(local) => {
            if let Some(init) = &local.init {
                let name = pat_to_ident(&local.pat)?;
                let value = lower_expr(&init.expr, ctx)?;
                Ok(Expression::Call {
                    function: "__crush_let__".to_string(),
                    args: vec![
                        Expression::Var {
                            name,
                            meta: ctx.meta_at(0),
                        },
                        value,
                    ],
                    meta: ctx.meta_at(0),
                })
            } else {
                Ok(Expression::NullLiteral {
                    meta: ctx.meta_at(0),
                })
            }
        }
        syn::Stmt::Item(_) => anyhow::bail!("item in block not supported"),
    }
}

pub fn pat_to_ident(pat: &syn::Pat) -> anyhow::Result<String> {
    match pat {
        syn::Pat::Ident(pi) => Ok(pi.ident.to_string()),
        syn::Pat::Type(pt) => pat_to_ident(&pt.pat),
        _ => anyhow::bail!("expected identifier pattern"),
    }
}

fn lower_block_to_stmts(block: &syn::Block, ctx: &LowerCtx<'_>) -> anyhow::Result<Vec<Statement>> {
    use crate::lower_stmt::lower_stmt;
    let mut stmts = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Expr(expr, _) => {
                let meta = ctx.meta_at(0);
                let expr = lower_expr(expr, ctx)?;
                stmts.push(Statement::ExprStmt { expr, meta });
            }
            syn::Stmt::Local(local) => {
                stmts.push(lower_stmt(&syn::Stmt::Local(local.clone()), ctx)?);
            }
            syn::Stmt::Item(item) => {
                stmts.push(lower_stmt(&syn::Stmt::Item(item.clone()), ctx)?);
            }
            syn::Stmt::Macro(m) => {
                stmts.push(lower_stmt(&syn::Stmt::Macro(m.clone()), ctx)?);
            }
        }
    }
    Ok(stmts)
}

fn lower_lit(
    lit: &syn::Lit,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Expression> {
    match lit {
        syn::Lit::Int(i) => {
            let val = i.base10_parse::<i64>()?;
            Ok(Expression::IntLiteral { value: val, meta })
        }
        syn::Lit::Float(f) => Ok(Expression::FloatLiteral {
            value: f.base10_parse()?,
            meta,
        }),
        syn::Lit::Str(s) => Ok(Expression::StringLiteral {
            value: s.value(),
            meta,
        }),
        syn::Lit::Bool(b) => Ok(Expression::BoolLiteral {
            value: b.value,
            meta,
        }),
        syn::Lit::Char(c) => Ok(Expression::StringLiteral {
            value: c.value().to_string(),
            meta,
        }),
        syn::Lit::Byte(b) => Ok(Expression::IntLiteral {
            value: b.value() as i64,
            meta,
        }),
        _ => {
            eprintln!("unsupported literal: {:#?}", lit);
            anyhow::bail!("unsupported literal")
        }
    }
}
