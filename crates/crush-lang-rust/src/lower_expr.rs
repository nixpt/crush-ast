//! syn Expression → CAST Expression lowering.

use std::collections::HashMap;

use crush_cast::Expression;
use syn::Expr;

pub fn lower_expr(expr: &Expr) -> anyhow::Result<Expression> {
    let meta = HashMap::new();
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
            let left = lower_expr(&e.left)?;
            let right = lower_expr(&e.right)?;
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
            let operand = lower_expr(&e.expr)?;
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
                .map(|a| lower_expr(a))
                .collect::<Result<Vec<_>, _>>()?;
            let func_name = match e.func.as_ref() {
                Expr::Path(p) => p
                    .path
                    .get_ident()
                    .ok_or_else(|| anyhow::anyhow!("complex call path"))?
                    .to_string(),
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
        Expr::If(e) => {
            let condition = lower_expr(&e.cond)?;
            // Lower then_branch (a Block) as expression
            let then_body = block_to_expr(&e.then_branch)?;
            let else_body = match &e.else_branch {
                Some((_, else_expr)) => lower_expr(else_expr)?,
                None => Expression::NullLiteral {
                    meta: HashMap::new(),
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
                    meta: HashMap::new(),
                });
            }
            let first = &e.block.stmts[0];
            match first {
                syn::Stmt::Expr(expr, _) => lower_expr(expr),
                syn::Stmt::Macro(_) => anyhow::bail!("macro in expression not supported"),
                syn::Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        let name = pat_to_ident(&local.pat)?;
                        let value = lower_expr(&init.expr)?;
                        Ok(Expression::Call {
                            function: "__crush_let__".to_string(),
                            args: vec![
                                Expression::Var {
                                    name,
                                    meta: HashMap::new(),
                                },
                                value,
                            ],
                            meta: HashMap::new(),
                        })
                    } else {
                        Ok(Expression::NullLiteral {
                            meta: HashMap::new(),
                        })
                    }
                }
                syn::Stmt::Item(_) => anyhow::bail!("item in block expression not supported"),
            }
        }
        Expr::Paren(e) => lower_expr(&e.expr),
        Expr::Return(e) => {
            let value = match &e.expr {
                Some(expr) => lower_expr(expr)?,
                None => Expression::NullLiteral {
                    meta: HashMap::new(),
                },
            };
            Ok(Expression::Call {
                function: "__crush_return__".to_string(),
                args: vec![value],
                meta,
            })
        }
        _ => anyhow::bail!("unsupported Rust expression"),
    }
}

fn block_to_expr(block: &syn::Block) -> anyhow::Result<Expression> {
    if block.stmts.is_empty() {
        return Ok(Expression::NullLiteral {
            meta: HashMap::new(),
        });
    }
    match &block.stmts[0] {
        syn::Stmt::Expr(expr, _) => lower_expr(expr),
        syn::Stmt::Macro(_) => anyhow::bail!("macro in block not supported"),
        syn::Stmt::Local(local) => {
            if let Some(init) = &local.init {
                let name = pat_to_ident(&local.pat)?;
                let value = lower_expr(&init.expr)?;
                Ok(Expression::Call {
                    function: "__crush_let__".to_string(),
                    args: vec![
                        Expression::Var {
                            name,
                            meta: HashMap::new(),
                        },
                        value,
                    ],
                    meta: HashMap::new(),
                })
            } else {
                Ok(Expression::NullLiteral {
                    meta: HashMap::new(),
                })
            }
        }
        syn::Stmt::Item(_) => anyhow::bail!("item in block not supported"),
    }
}

pub fn pat_to_ident(pat: &syn::Pat) -> anyhow::Result<String> {
    match pat {
        syn::Pat::Ident(pi) => Ok(pi.ident.to_string()),
        _ => anyhow::bail!("expected identifier pattern"),
    }
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
        _ => anyhow::bail!("unsupported literal"),
    }
}
