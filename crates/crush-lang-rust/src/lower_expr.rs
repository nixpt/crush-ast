//! syn Expression → CAST Expression lowering.

use std::collections::HashMap;

use crush_walker_core_no_tree_sitter::LowerCtx;
use quote::ToTokens;

use crush_cast::{CastType, Expression, Statement};
use syn::Expr;
use syn::spanned::Spanned;

pub fn lower_expr(expr: &Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    let start = expr.span().start();
    lower_expr_inner(expr, ctx).map_err(|error| {
        anyhow::anyhow!("Rust expression at {}:{}: {error:#}", start.line, start.column + 1)
    })
}

fn lower_expr_inner(expr: &Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    let start = expr.span().start();
    let meta = ctx.meta_lc(start.line, start.column + 1);
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
                    let mut meta = meta;
                    meta.insert("rust_compound_assign".into(), serde_json::Value::Bool(true));
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
            if matches!(e.op, syn::UnOp::Neg(_))
                && let Expr::Lit(literal) = e.expr.as_ref()
                && let syn::Lit::Int(integer) = &literal.lit
            {
                return crate::numeric::lower_literal(integer, true, meta);
            }
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
            let mut meta = meta;
            meta.insert("rust_source_call".to_string(), serde_json::Value::Bool(true));
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
            meta.insert("rust_source_name".to_string(), serde_json::Value::String(func_name.clone()));
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
            block_to_expr(&e.block, ctx)
        }
        Expr::Paren(e) => lower_expr(&e.expr, ctx),
        Expr::Return(e) => {
            Err(unsupported_at(e.span(), "return in a value expression",
                "use return as a function or closure statement"))
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
        Expr::Cast(e) => crate::numeric::lower_cast(e, meta),
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
            let mut meta = meta;
            meta.insert("rust_numeric_params".into(),
                serde_json::Value::Array(e.inputs.iter().map(crate::numeric::hint).collect()));
            // A closure's omitted return type is inferred, unlike a free
            // function's omitted return type (unit).
            meta.insert("rust_numeric_return".into(), match e.output {
                syn::ReturnType::Default => serde_json::Value::Null,
                _ => crate::numeric::result(&e.output),
            });
            meta.insert("rust_value_params".into(),
                serde_json::Value::Array(e.inputs.iter().map(crate::values::hint).collect()));
            if e.capture.is_some() {
                return Err(unsupported_at(e.span(), "move closure",
                    "value capture requires Rust ownership semantics (CRUSH-xps-7)"));
            }
            if e.asyncness.is_some() {
                return Err(unsupported_at(e.span(), "async closure",
                    "async Rust execution is not implemented"));
            }
            let parameter_names: std::collections::HashSet<_> = e.inputs.iter()
                .filter_map(|p| pat_to_ident(p).ok()).collect();
            let params: Vec<(String, CastType)> = e.inputs.iter()
                .enumerate()
                .map(|(i, p)| {
                    let name = closure_param_name(p, i, &parameter_names)
                        .map_err(|err| anyhow::anyhow!("unsupported Rust closure parameter: {err}"))?;
                    Ok((name, CastType::Any))
                })
                .collect::<anyhow::Result<_>>()?;
            // A lambda has a statement body in CAST. Reuse function-tail
            // lowering so all statements and explicit returns are preserved.
            let body = crate::lower_stmt::lower_tail_return(&e.body, ctx)?;
            Ok(Expression::Lambda {
                params,
                body,
                meta,
            })
        }
        Expr::Reference(e) => {
            let mut meta = meta;
            meta.insert("rust_value_method".into(), serde_json::json!("reference"));
            Ok(Expression::Call {
                function: "__crush_rust_value__".into(),
                args: vec![lower_expr(&e.expr, ctx)?], meta,
            })
        }
        Expr::Macro(m) => lower_macro_expr(&m.mac, ctx),
        Expr::Match(e) => lower_match(e, ctx),
        _ => anyhow::bail!("unsupported Rust expression `{}`; this construct has no CAST lowering", expr.to_token_stream()),
    }
}

fn block_to_expr(block: &syn::Block, ctx: &LowerCtx<'_>) -> anyhow::Result<Expression> {
    match block.stmts.as_slice() {
        [] => {
            let start = block.span().start();
            Ok(Expression::NullLiteral { meta: ctx.meta_lc(start.line, start.column + 1) })
        }
        [syn::Stmt::Expr(expr, None)] => lower_expr(expr, ctx),
        [syn::Stmt::Macro(mac)] if mac.semi_token.is_none() => lower_macro_expr(&mac.mac, ctx),
        _ => Err(unsupported_at(block.span(), "block expression",
            "only an empty block or a single tail expression is supported in a value position; multi-statement and semicolon-ending blocks need statement lowering")),
    }
}

fn unsupported_at(span: proc_macro2::Span, construct: &str, reason: &str) -> anyhow::Error {
    let start = span.start();
    anyhow::anyhow!("unsupported Rust {construct} at {}:{}: {reason}", start.line, start.column + 1)
}

pub fn pat_to_ident(pat: &syn::Pat) -> anyhow::Result<String> {
    match pat {
        syn::Pat::Ident(pi) => Ok(pi.ident.to_string()),
        syn::Pat::Type(pt) => pat_to_ident(&pt.pat),
        _ => anyhow::bail!("expected identifier pattern"),
    }
}

fn closure_param_name(
    pat: &syn::Pat,
    index: usize,
    parameter_names: &std::collections::HashSet<String>,
) -> anyhow::Result<String> {
    match pat {
        syn::Pat::Type(typed) => closure_param_name(&typed.pat, index, parameter_names),
        syn::Pat::Ident(ident) if ident.by_ref.is_none() && ident.subpat.is_none() => {
            Ok(ident.ident.to_string())
        }
        syn::Pat::Wild(_) => {
            let mut name = format!("__closure_unused_{index}");
            while parameter_names.contains(&name) {
                name.push('_');
            }
            Ok(name)
        }
        _ => anyhow::bail!("destructuring and ref patterns are not supported; expected an identifier or wildcard"),
    }
}

fn lower_lit(
    lit: &syn::Lit,
    meta: HashMap<String, serde_json::Value>,
) -> anyhow::Result<Expression> {
    match lit {
        syn::Lit::Int(i) => crate::numeric::lower_literal(i, false, meta),
        syn::Lit::Float(f) => {
            anyhow::ensure!(f.suffix().is_empty() || f.suffix() == "f64",
                "unsupported Rust float suffix `{}`: f32 precision is not implemented (CRUSH-xps-8)", f.suffix());
            let mut meta = meta;
            meta.insert("rust_numeric_suffix".into(), serde_json::json!(f.suffix()));
            Ok(Expression::FloatLiteral { value: f.base10_parse()?, meta })
        }
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
        syn::Lit::Byte(b) => {
            let mut meta = meta;
            meta.insert("rust_numeric_suffix".into(), serde_json::json!("u8"));
            Ok(Expression::IntLiteral { value: b.value() as i64, meta })
        }
        _ => {
            anyhow::bail!("unsupported Rust literal `{}`; this literal has no CAST representation", lit.to_token_stream())
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
            let mut meta = meta;
            meta.insert("rust_vec".into(), serde_json::Value::Bool(true));
            Ok(Expression::ArrayLiteral { elements, meta })
        }
        "format" => format_args_to_expr(&mac.tokens, ctx),
        "println" | "print" | "eprintln" | "eprint" => {
            crate::formatting::output_macro(mac)?;
            Ok(Expression::CapabilityCall {
            name: "io.print".to_string(),
            args: vec![format_args_to_expr(&mac.tokens, ctx)?],
            meta,
        }) },
        _ => anyhow::bail!("macro invocation not supported: {name}!"),
    }
}

/// `"fmt", args..` → a string-concatenation expression (CRUSH-197).
///
/// Supports Display `{}`, typed literal Debug `{:?}`, positional `{0}`, inline names
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
    if args.iter().any(|e| matches!(e, Expr::Assign(_))) {
        return Err(crate::formatting::error(&parts, "named format arguments need explicit argument materialization"));
    }
    let mut uses = Vec::new();
    let mut captures = false;
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
                let argument = if target.is_empty() {
                    let index = next_arg;
                    next_arg += 1;
                    Some(index)
                } else {
                    target.parse::<usize>().ok()
                };
                let value = if let Some(i) = argument {
                    let e = args.get(i).ok_or_else(|| anyhow::anyhow!("format argument {i} missing"))?;
                    uses.push(i);
                    if fmt_spec == "?" { crate::formatting::debug(e, ctx)? } else { lower_expr(e, ctx)? }
                } else {
                    captures = true;
                    if fmt_spec == "?" {
                        return Err(crate::formatting::error(&parts, "Debug requires a proven typed literal; dynamic values need the shared formatter"));
                    }
                    Expression::Var { name: target, meta: ctx.meta_at(0) }
                };
                if !lit.is_empty() {
                    pieces.push(Expression::StringLiteral { value: std::mem::take(&mut lit), meta: ctx.meta_at(0) });
                }
                // Display uses its existing two-argument ABI. Literal Debug
                // has already produced exact typed text above.
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
    if (0..args.len()).any(|i| !uses.contains(&i)) {
        return Err(crate::formatting::error(&parts, "unused format arguments are unsupported"));
    }
    if args.iter().any(|e| !crate::formatting::inert(e))
        && (captures || uses != (0..args.len()).collect::<Vec<_>>())
    {
        return Err(crate::formatting::error(&parts, "effectful repeated/reordered format arguments need explicit argument materialization"));
    }
    // Start from "" so `+` is string concatenation even when the first piece is a number.
    let mut acc = Expression::StringLiteral { value: String::new(), meta: ctx.meta_at(0) };
    for p in pieces {
        let mut meta = ctx.meta_at(0);
        meta.insert("rust_format_piece".into(), serde_json::Value::Bool(true));
        acc = Expression::BinaryOp { operator: "+".to_string(), left: Box::new(acc), right: Box::new(p), meta };
    }
    // Even literal-only/empty format! produces an owned String. Keeping a
    // format concatenation node lets the ownership pass retain that fact.
    let mut meta = ctx.meta_at(0);
    meta.insert("rust_format_piece".into(), serde_json::json!(true));
    Ok(Expression::BinaryOp { operator: "+".into(), left: Box::new(acc),
        right: Box::new(Expression::StringLiteral { value: String::new(), meta: meta.clone() }), meta })
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
    // `len` is a compiler builtin; mapped standard-library methods use the
    // named capability path, just as parsed Crush source does. A dotted Call
    // bypassed that contract and failed semantic analysis as undefined.
    let call = |name: &str, args: Vec<Expression>, mut meta: HashMap<String, serde_json::Value>| {
        if name == "len" {
            Expression::Call { function: name.to_string(), args, meta }
        } else {
            // This is an already-resolved capability, even if the Rust source
            // has a local binding named `str`, `math`, or `array`.
            meta.insert("resolved_capability".into(), serde_json::json!(true));
            Expression::CapabilityCall { name: name.to_string(), args, meta }
        }
    };
    let mut meta = meta;
    meta.insert("rust_value_method".into(), serde_json::json!(method));
    Ok(match (method, args.len()) {
        // The value pass proves and rewrites ownership/iterator adapters.
        ("iter" | "into_iter" | "clone" | "to_owned" | "as_str" | "copied" | "cloned" | "to_vec" | "into"
            | "unwrap" | "as_slice" | "borrow", 1) => call("__crush_rust_value__", args, meta),
        ("to_string", 1) => call("conv.text", vec![args.remove(0),
            Expression::StringLiteral { value: "rust".into(), meta: meta.clone() }], meta),
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
