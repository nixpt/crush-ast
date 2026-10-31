//! Rust Debug lowering: typed literal folding and pending runtime calls.
//! Lexical descriptor proofs resolve dynamic values before ownership checking;
//! runtime string representation is never used to guess Rust types.
use crush_cast::Expression as E;
use crush_walker_core_no_tree_sitter::LowerCtx;
use syn::{Expr, spanned::Spanned};

mod types;
pub(crate) use types::resolve;

pub(crate) fn pending(expr: &Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<E> {
    let p = expr.span().start();
    let mut meta = ctx.meta_lc(p.line, p.column + 1);
    meta.insert("rust_debug_pending".into(), serde_json::json!(true));
    Ok(E::CapabilityCall {
        name: "rust.debug".into(),
        args: vec![
            crate::lower_expr::lower_expr(expr, ctx)?,
            E::NullLiteral { meta: meta.clone() },
        ],
        meta,
    })
}

fn literal_tree(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(_) => true,
        Expr::Paren(e) => literal_tree(&e.expr),
        Expr::Group(e) => literal_tree(&e.expr),
        Expr::Unary(e) => literal_tree(&e.expr),
        Expr::Cast(e) => literal_tree(&e.expr),
        Expr::Array(a) => a.elems.iter().all(literal_tree),
        Expr::Tuple(t) if t.elems.is_empty() => true,
        _ => false,
    }
}

const MAX_DEPTH: usize = 16;
const MAX_ITEMS: usize = 256;
const MAX_OUTPUT: usize = 1024 * 1024;

pub(crate) fn error(expr: &impl Spanned, reason: &str) -> anyhow::Error {
    crate::lower_expr::unsupported_at(
        expr.span(),
        "formatting",
        &format!("{reason} (CRUSH-xps-9)"),
    )
}

pub(crate) fn output_macro(mac: &syn::Macro) -> anyhow::Result<()> {
    if !mac.path.is_ident("println") {
        return Err(error(
            mac,
            "print!/eprint!/eprintln! need faithful newline/channel routing",
        ));
    }
    Ok(())
}

pub(crate) fn inert(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(_) | Expr::Path(_) => true,
        Expr::Paren(e) => inert(&e.expr),
        Expr::Group(e) => inert(&e.expr),
        _ => false,
    }
}

pub(crate) fn debug(expr: &Expr, ctx: &LowerCtx<'_>) -> anyhow::Result<E> {
    if !literal_tree(expr) {
        return pending(expr, ctx);
    }
    fn render(
        expr: &Expr,
        ctx: &LowerCtx<'_>,
        depth: usize,
        items: &mut usize,
    ) -> anyhow::Result<String> {
        *items += 1;
        if depth > MAX_DEPTH || *items > MAX_ITEMS {
            return Err(error(
                expr,
                "literal Debug traversal exceeds its depth/item bound",
            ));
        }
        let text = match expr {
            Expr::Paren(e) => render(&e.expr, ctx, depth + 1, items)?,
            Expr::Group(e) => render(&e.expr, ctx, depth + 1, items)?,
            Expr::Tuple(e) if e.elems.is_empty() => "()".into(),
            Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => format!("{:?}", s.value()),
            Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Char(c),
                ..
            }) => format!("{:?}", c.value()),
            Expr::Array(a) => {
                let mut text = String::from("[");
                for (n, e) in a.elems.iter().enumerate() {
                    if n > 0 {
                        text.push_str(", ");
                    }
                    text.push_str(&render(e, ctx, depth + 1, items)?);
                    if text.len() > MAX_OUTPUT {
                        return Err(error(expr, "literal Debug output exceeds its byte bound"));
                    }
                }
                text.push(']');
                text
            }
            Expr::Unary(e)
                if matches!(e.op, syn::UnOp::Neg(_))
                    && matches!(
                        e.expr.as_ref(),
                        Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Float(_),
                            ..
                        })
                    ) =>
            {
                match crate::lower_expr::lower_expr(&e.expr, ctx)? {
                    E::FloatLiteral { value, .. } if value.is_finite() => format!("{:?}", -value),
                    _ => {
                        return Err(error(
                            expr,
                            "Debug float literal needs a finite representable value",
                        ));
                    }
                }
            }
            Expr::Lit(_) | Expr::Unary(_) | Expr::Cast(_) => {
                match crate::lower_expr::lower_expr(expr, ctx)? {
                    E::IntLiteral { value, .. } => {
                        let unsuffixed = match expr {
                            Expr::Lit(syn::ExprLit {
                                lit: syn::Lit::Int(n),
                                ..
                            }) => n.suffix().is_empty(),
                            Expr::Unary(e) => {
                                matches!(e.expr.as_ref(), Expr::Lit(syn::ExprLit { lit: syn::Lit::Int(n), .. }) if n.suffix().is_empty())
                            }
                            _ => false,
                        };
                        if unsuffixed && i32::try_from(value).is_err() {
                            return Err(error(
                                expr,
                                "unsuffixed Debug integer needs a proven source width",
                            ));
                        }
                        format!("{value:?}")
                    }
                    E::FloatLiteral { value, .. } if value.is_finite() => format!("{value:?}"),
                    E::BoolLiteral { value, .. } => format!("{value:?}"),
                    _ => {
                        return Err(error(
                            expr,
                            "Debug requires a proven typed literal; dynamic values need the shared formatter",
                        ));
                    }
                }
            }
            _ => {
                return Err(error(
                    expr,
                    "Debug requires a proven typed literal; dynamic values need the shared formatter",
                ));
            }
        };
        if text.len() > MAX_OUTPUT {
            return Err(error(expr, "literal Debug output exceeds its byte bound"));
        }
        Ok(text)
    }
    // Count inside casts/unary expressions too; folding them must not bypass
    // the traversal budget enforced on array/parenthesized literals.
    fn budget(expr: &Expr, depth: usize, nodes: &mut usize) -> anyhow::Result<()> {
        *nodes += 1;
        if depth > MAX_DEPTH || *nodes > MAX_ITEMS {
            return Err(error(
                expr,
                "literal Debug traversal exceeds its depth/item bound",
            ));
        }
        match expr {
            Expr::Paren(e) => budget(&e.expr, depth + 1, nodes)?,
            Expr::Group(e) => budget(&e.expr, depth + 1, nodes)?,
            Expr::Unary(e) => budget(&e.expr, depth + 1, nodes)?,
            Expr::Cast(e) => budget(&e.expr, depth + 1, nodes)?,
            Expr::Array(a) => {
                for e in &a.elems {
                    budget(e, depth + 1, nodes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    budget(expr, 0, &mut 0)?;
    let p = expr.span().start();
    let mut meta = ctx.meta_lc(p.line, p.column + 1);
    meta.insert("rust_debug_literal".into(), serde_json::json!(true));
    Ok(E::StringLiteral {
        value: render(expr, ctx, 0, &mut 0)?,
        meta,
    })
}
