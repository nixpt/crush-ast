//! syn Expression → CAST Expression lowering.

use std::collections::HashMap;

use crush_walker_core_no_tree_sitter::LowerCtx;
use quote::ToTokens;

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
                .ok_or_else(|| anyhow::anyhow!(
                    "Rust path `{}` cannot be resolved by the Crush walker; Rust modules/crates must be exposed through an explicit Crush capability/plugin",
                    e.path.to_token_stream()
                ))?
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
                syn::BinOp::AddAssign(_) | syn::BinOp::SubAssign(_) | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_) | syn::BinOp::RemAssign(_) => {
                    let operator = match e.op {
                        syn::BinOp::AddAssign(_) => "+",
                        syn::BinOp::SubAssign(_) => "-",
                        syn::BinOp::MulAssign(_) => "*",
                        syn::BinOp::DivAssign(_) => "/",
                        syn::BinOp::RemAssign(_) => "%",
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
                Expr::Path(p) => {
                    let name = p.path.to_token_stream().to_string();
                    if p.path.segments.len() != 1 {
                        anyhow::bail!(
                            "Rust call `{name}` cannot be resolved by the Crush walker; Rust modules/crates must be exposed through an explicit Crush capability/plugin"
                        );
                    }
                    name
                }
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
                function: "__crush_ternary__".to_string(), // lazy; `__crush_ifexpr__` was an unknown helper
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
            lower_method(&e.method.to_string(), args, meta)
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
                // CRUSH-197: `a..=b` is inclusive — the CAST range is half-open.
                Some(end) if matches!(e.limits, syn::RangeLimits::Closed(_)) => Expression::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(lower_expr(end, ctx)?),
                    right: Box::new(Expression::IntLiteral { value: 1, meta: ctx.meta_at(0) }),
                    meta: ctx.meta_at(0),
                },
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
            // A typed parameter (`|x: i32|`) is `Pat::Type` around the name. It
            // used to become a placeholder `_param`, so the body's `x` was a
            // free variable — captured from an enclosing `x` if there was one
            // (silently wrong: `let x = 5; (|x: i32| x + 1)(41)` gave 6).
            fn closure_param(p: &syn::Pat, i: usize) -> anyhow::Result<String> {
                match p {
                    syn::Pat::Ident(pi) => Ok(pi.ident.to_string()),
                    syn::Pat::Type(pt) => closure_param(&pt.pat, i),
                    syn::Pat::Wild(_) => Ok(format!("__closure_unused_{i}")),
                    _ => anyhow::bail!(
                        "closure parameter pattern `{}` (destructuring) is not supported yet",
                        quote::ToTokens::to_token_stream(p)
                    ),
                }
            }
            let params: Vec<(String, CastType)> = e.inputs.iter()
                .enumerate()
                .map(|(i, p)| Ok((closure_param(p, i)?, CastType::Any)))
                .collect::<anyhow::Result<_>>()?;
            // A block body is lowered like a function body — statements, then the
            // tail as the return value, `return` as a return. Lowered as an
            // expression it kept only the block's first statement, so
            // `|mut x: i32| { x += 1; x }` returned null.
            let body = match e.body.as_ref() {
                Expr::Block(b) => crate::lower_stmt::lower_block(
                    &b.block.stmts,
                    ctx,
                    crate::lower_stmt::Tail::Return,
                )?,
                other => vec![Statement::Return { value: Some(lower_expr(other, ctx)?), meta: meta.clone() }],
            };
            Ok(Expression::Lambda { params, body, meta })
        }
        Expr::Reference(e) => {
            lower_expr(&e.expr, ctx)
        }
        Expr::Macro(m) => lower_macro_expr(&m.mac, ctx),
        Expr::Match(e) => lower_match(e, ctx),
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

/// Expression-position macros (CRUSH-197): `vec![..]`, `format!(..)`, `println!(..)`.
fn lower_macro_expr(mac: &syn::Macro, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    let meta = ctx.meta_at(0);
    let name = mac.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
    match name.as_str() {
        "vec" => {
            use syn::parse::Parser;
            let elems = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated
                .parse2(mac.tokens.clone())
                .map_err(|_| anyhow::anyhow!("vec![value; n] (repeat form) is not supported"))?;
            let elements = elems.iter().map(|e| lower_expr(e, ctx)).collect::<Result<Vec<_>, _>>()?;
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        "format" => format_args_to_expr(&mac.tokens, ctx),
        "println" | "print" | "eprintln" | "eprint" => Ok(Expression::CapabilityCall {
            name: "io.print".to_string(),
            args: vec![format_args_to_expr(&mac.tokens, ctx)?],
            meta,
        }),
        _ => anyhow::bail!("macro invocation not supported: {name}!"),
    }
}

/// `"fmt", args..` → a string-concatenation expression (CRUSH-197).
///
/// Supports `{}`, `{:?}` (rendered like `{}`), positional `{0}`, inline names
/// `{x}` (Rust 2021) and `{{` / `}}`. Width/precision/fill specs (`{:.2}`,
/// `{:>5}`) are rejected loudly rather than printed wrong.
pub fn format_args_to_expr(tokens: &proc_macro2::TokenStream, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    use syn::parse::Parser;
    let meta = ctx.meta_at(0);
    let parts = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated
        .parse2(tokens.clone())
        .map_err(|e| anyhow::anyhow!("could not parse format arguments: {e}"))?;
    let mut it = parts.iter();
    let fmt = match it.next() {
        None => return Ok(Expression::StringLiteral { value: String::new(), meta }),
        Some(Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. })) => s.value(),
        Some(_) => anyhow::bail!("format string must be a string literal"),
    };
    let args: Vec<&Expr> = it.collect();
    let mut next_arg = 0usize;
    let mut pieces: Vec<Expression> = Vec::new();
    let mut lit = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => { chars.next(); lit.push('{'); }
            '}' if chars.peek() == Some(&'}') => { chars.next(); lit.push('}'); }
            '{' => {
                let mut spec = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(ch) => spec.push(ch),
                        None => anyhow::bail!("unterminated `{{` in format string"),
                    }
                }
                let (target, fmt_spec) = match spec.split_once(':') {
                    Some((t, f)) => (t.trim().to_string(), f.trim().to_string()),
                    None => (spec.trim().to_string(), String::new()),
                };
                if !(fmt_spec.is_empty() || fmt_spec == "?") {
                    anyhow::bail!("format spec `{{{spec}}}` is not supported yet (only {{}}, {{:?}}, {{name}}, {{0}})");
                }
                let value = if target.is_empty() {
                    let e = args.get(next_arg).ok_or_else(|| anyhow::anyhow!("format string has more `{{}}` than arguments"))?;
                    next_arg += 1;
                    lower_expr(e, ctx)?
                } else if let Ok(i) = target.parse::<usize>() {
                    lower_expr(args.get(i).ok_or_else(|| anyhow::anyhow!("format argument {i} missing"))?, ctx)?
                } else {
                    Expression::Var { name: target, meta: ctx.meta_at(0) }
                };
                if !lit.is_empty() {
                    pieces.push(Expression::StringLiteral { value: std::mem::take(&mut lit), meta: ctx.meta_at(0) });
                }
                // `{}` is Display: floats print like rustc (`6`, never `6.0`,
                // no exponent). `{:?}` is Debug (`6.0`), which is how crush
                // already prints floats.
                pieces.push(if fmt_spec.is_empty() {
                    Expression::CapabilityCall {
                        name: "conv.text".to_string(),
                        args: vec![value, Expression::StringLiteral { value: "rust".to_string(), meta: ctx.meta_at(0) }],
                        meta: ctx.meta_at(0),
                    }
                } else {
                    value
                });
            }
            '}' => anyhow::bail!("unmatched `}}` in format string"),
            other => lit.push(other),
        }
    }
    if !lit.is_empty() {
        pieces.push(Expression::StringLiteral { value: lit, meta: ctx.meta_at(0) });
    }
    // Start from "" so `+` is string concatenation even when the first piece is a number.
    let mut acc = Expression::StringLiteral { value: String::new(), meta: ctx.meta_at(0) };
    for p in pieces {
        acc = Expression::BinaryOp { operator: "+".to_string(), left: Box::new(acc), right: Box::new(p), meta: ctx.meta_at(0) };
    }
    Ok(acc)
}

/// `match x { 1 | 2 => a, n => b, _ => c }` → CAST `Match` (CRUSH-197).
fn lower_match(e: &syn::ExprMatch, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    use crate::lower_stmt::{lower_block, Tail};
    let mut arms = Vec::new();
    for arm in &e.arms {
        if arm.guard.is_some() {
            anyhow::bail!("match guards (`if` in a match arm) are not supported yet");
        }
        let body = match arm.body.as_ref() {
            Expr::Block(b) => lower_block(&b.block.stmts, ctx, Tail::Value)?,
            other => vec![Statement::ExprStmt { expr: lower_expr(other, ctx)?, meta: ctx.meta_at(0) }],
        };
        for pattern in lower_pattern(&arm.pat, ctx)? {
            arms.push(crush_cast::MatchArm { pattern, body: body.clone() });
        }
    }
    Ok(Expression::Match { expression: Box::new(lower_expr(&e.expr, ctx)?), arms, meta: ctx.meta_at(0) })
}

/// A Rust pattern → CAST patterns (or-patterns expand to one arm each).
fn lower_pattern(p: &syn::Pat, ctx: &LowerCtx<'_>) -> anyhow::Result<Vec<crush_cast::Pattern>> {
    use crush_cast::Pattern;
    Ok(match p {
        syn::Pat::Wild(_) => vec![Pattern::Wildcard],
        syn::Pat::Lit(l) => vec![Pattern::Literal { value: lower_lit(&l.lit, ctx.meta_at(0))? }],
        syn::Pat::Ident(pi) if pi.subpat.is_none() => vec![Pattern::Identifier { name: pi.ident.to_string() }],
        syn::Pat::Or(o) => {
            let mut out = Vec::new();
            for case in &o.cases {
                out.extend(lower_pattern(case, ctx)?);
            }
            out
        }
        syn::Pat::Paren(pp) => lower_pattern(&pp.pat, ctx)?,
        // A negative literal (`-1 =>`) parses as a unary expression pattern.
        syn::Pat::Range(_) => anyhow::bail!("range patterns (`1..=5 =>`) are not supported yet"),
        other => anyhow::bail!("unsupported match pattern: {}", quote_pat(other)),
    })
}

fn quote_pat(p: &syn::Pat) -> String {
    use quote::ToTokens;
    p.to_token_stream().to_string()
}

/// Rust method calls → crush builtins (CRUSH-197). `args[0]` is the receiver.
fn lower_method(
    method: &str,
    mut args: Vec<Expression>,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Expression> {
    let call = |function: &str, args: Vec<Expression>, meta| Expression::Call { function: function.to_string(), args, meta };
    Ok(match (method, args.len()) {
        // Borrow / ownership / iterator adapters carry no runtime meaning here.
        ("iter" | "into_iter" | "clone" | "to_owned" | "as_str" | "copied" | "cloned" | "to_vec" | "into"
            | "unwrap" | "as_slice" | "borrow", 1) => args.remove(0),
        ("to_string", 1) => Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(Expression::StringLiteral { value: String::new(), meta: meta.clone() }),
            right: Box::new(args.remove(0)),
            meta,
        },
        ("len", 1) => call("len", args, meta),
        ("push", 2) => call("array.push", args, meta),
        ("pop", 1) => call("array.pop", args, meta),
        ("contains", 2) => call("str.contains", args, meta),
        ("starts_with", 2) => call("str.starts_with", args, meta),
        ("ends_with", 2) => call("str.ends_with", args, meta),
        ("to_uppercase", 1) => call("str.to_upper", args, meta),
        ("to_lowercase", 1) => call("str.to_lower", args, meta),
        ("trim", 1) => call("str.trim", args, meta),
        ("replace", 3) => call("str.replace", args, meta),
        ("abs", 1) => call("math.abs", args, meta),
        ("sqrt", 1) => call("math.sqrt", args, meta),
        ("floor", 1) => call("math.floor", args, meta),
        ("ceil", 1) => call("math.ceil", args, meta),
        ("round", 1) => call("math.round", args, meta),
        ("pow" | "powi" | "powf", 2) => call("math.pow", args, meta),
        (other, n) => anyhow::bail!("method `.{other}()` with {} argument(s) is not supported", n - 1),
    })
}
