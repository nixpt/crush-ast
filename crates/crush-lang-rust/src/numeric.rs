//! The first xps-8 slice: retain source hints and lower literal integer casts.
//!
//! These are syntax facts, not inferred types. In particular, an unsuffixed
//! literal's runtime Int representation does not prove that its Rust type is
//! i64. Binding inference and typed arithmetic are subsequent xps-8 slices.

use std::collections::HashMap;

use crush_cast::Expression;
use quote::ToTokens;
use serde_json::{Value, json};

type Meta = HashMap<String, Value>;

#[derive(Clone, Copy)]
struct Integer {
    name: &'static str,
    bits: u32,
    signed: bool,
}

impl Integer {
    fn parse(name: &str) -> anyhow::Result<Self> {
        let (name, bits, signed) = match name {
            "i8" => ("i8", 8, true),
            "i16" => ("i16", 16, true),
            "i32" => ("i32", 32, true),
            "i64" => ("i64", 64, true),
            "u8" => ("u8", 8, false),
            "u16" => ("u16", 16, false),
            "u32" => ("u32", 32, false),
            _ => anyhow::bail!(
                "unsupported Rust integer type `{name}`: supported cast/literal widths are i8/i16/i32/i64 and u8/u16/u32 (CRUSH-xps-8)"
            ),
        };
        Ok(Self { name, bits, signed })
    }

    fn check(self, value: i128) -> anyhow::Result<i128> {
        let (min, max) = if self.signed {
            (-(1i128 << (self.bits - 1)), (1i128 << (self.bits - 1)) - 1)
        } else {
            (0, (1i128 << self.bits) - 1)
        };
        anyhow::ensure!(
            (min..=max).contains(&value),
            "Rust integer literal {value} is outside `{}` (CRUSH-xps-8)",
            self.name
        );
        Ok(value)
    }

    fn convert(self, value: i128) -> i128 {
        // Host-side constant evaluation only. Every result fits the existing
        // guest i64 Int; no runtime wide integer or backend helper is introduced.
        let modulus = 1i128 << self.bits;
        let low = value & (modulus - 1);
        if self.signed && low >= modulus / 2 {
            low - modulus
        } else {
            low
        }
    }
}

fn magnitude(literal: &syn::LitInt, negative: bool) -> anyhow::Result<i128> {
    let value = literal.base10_parse::<i128>()?;
    if negative && literal.suffix().starts_with('u') {
        anyhow::bail!("unary minus on an unsigned Rust literal is unsupported (CRUSH-xps-8)");
    }
    Ok(if negative { -value } else { value })
}

pub(crate) fn lower_literal(
    literal: &syn::LitInt,
    negative: bool,
    mut meta: Meta,
) -> anyhow::Result<Expression> {
    let explicit = if literal.suffix().is_empty() {
        None
    } else {
        Some(Integer::parse(literal.suffix())?)
    };
    let mut value = magnitude(literal, negative)?;
    if let Some(ty) = explicit {
        value = ty.check(value)?;
    }
    let value = i64::try_from(value).map_err(|_| {
        anyhow::anyhow!("Rust integer literal is not representable in Crush Int (CRUSH-xps-8)")
    })?;
    // An empty suffix deliberately remains unresolved, including for literals
    // whose surrounding parameter/annotation will eventually infer their type.
    meta.insert("rust_numeric_suffix".into(), json!(literal.suffix()));
    Ok(Expression::IntLiteral { value, meta })
}

fn target(ty: &syn::Type) -> anyhow::Result<Integer> {
    match ty {
        syn::Type::Paren(p) => target(&p.elem),
        syn::Type::Group(g) => target(&g.elem),
        syn::Type::Path(p) if p.qself.is_none() => {
            if let Some(ident) = p.path.get_ident() {
                return Integer::parse(&ident.to_string());
            }
            anyhow::bail!(
                "unsupported Rust cast target `{}` (CRUSH-xps-8)",
                ty.to_token_stream()
            )
        }
        _ => anyhow::bail!(
            "unsupported Rust cast target `{}`: only bounded integer casts are implemented (CRUSH-xps-8)",
            ty.to_token_stream()
        ),
    }
}

fn constant(expr: &syn::Expr, inferred: Integer) -> anyhow::Result<(i128, Integer)> {
    let (literal, negative) = match expr {
        syn::Expr::Paren(p) => return constant(&p.expr, inferred),
        syn::Expr::Group(g) => return constant(&g.expr, inferred),
        syn::Expr::Cast(c) => {
            let to = target(&c.ty)?;
            let (value, _) = constant(&c.expr, to)?;
            return Ok((to.convert(value), to));
        }
        syn::Expr::Lit(l) => (&l.lit, false),
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) => {
            // The minimum signed literal is legal even though its positive
            // magnitude does not fit the signed source type.
            let syn::Expr::Lit(l) = u.expr.as_ref() else {
                anyhow::bail!(
                    "nonliteral Rust cast operand needs numeric binding/range analysis (CRUSH-xps-8)"
                );
            };
            (&l.lit, true)
        }
        _ => anyhow::bail!(
            "nonliteral Rust cast operand needs numeric binding/range analysis (CRUSH-xps-8)"
        ),
    };
    let syn::Lit::Int(literal) = literal else {
        anyhow::bail!("noninteger Rust cast operand is unsupported (CRUSH-xps-8)");
    };
    // A known integer cast target constrains its unsuffixed literal operand.
    // For example `255 as i8` is an overflowing i8 literal, while `255u8 as i8`
    // is a valid truncating conversion. Do not apply unconstrained i32 fallback.
    let from = if literal.suffix().is_empty() {
        inferred
    } else {
        Integer::parse(literal.suffix())?
    };
    anyhow::ensure!(
        !negative || from.signed,
        "unary minus on an unsigned Rust literal is unsupported (CRUSH-xps-8)"
    );
    Ok((from.check(magnitude(literal, negative)?)?, from))
}

pub(crate) fn lower_cast(cast: &syn::ExprCast, mut meta: Meta) -> anyhow::Result<Expression> {
    let to = target(&cast.ty)?;
    let (value, from) = constant(&cast.expr, to)?;
    meta.insert("rust_numeric_suffix".into(), json!(to.name));
    meta.insert("rust_integer_cast_from".into(), json!(from.name));
    Ok(Expression::IntLiteral {
        value: i64::try_from(to.convert(value))?,
        meta,
    })
}

/// Preserve annotations independently of the ownership pass's `Kind::Copy`.
/// Consumers must interpret these source hints before authorizing arithmetic.
pub(crate) fn hint(pat: &syn::Pat) -> Value {
    match pat {
        syn::Pat::Type(p) => json!(p.ty.to_token_stream().to_string()),
        _ => Value::Null,
    }
}

pub(crate) fn result(ret: &syn::ReturnType) -> Value {
    match ret {
        syn::ReturnType::Default => json!("()"),
        syn::ReturnType::Type(_, ty) => json!(ty.to_token_stream().to_string()),
    }
}

pub(crate) fn signature(sig: &syn::Signature) -> (Value, Value) {
    let params: Vec<_> = sig
        .inputs
        .iter()
        .map(|p| match p {
            syn::FnArg::Typed(p) => json!(p.ty.to_token_stream().to_string()),
            _ => Value::Null,
        })
        .collect();
    (json!(params), result(&sig.output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casts_honor_source_suffixes_and_literal_target_inference() {
        for (source, expected, suffix) in [
            ("300i64 as u8", 44, "u8"),
            ("-1i16 as u8", 255, "u8"),
            ("255u8 as i8", -1, "i8"),
            ("3_000_000_000i64 as i32", -1294967296, "i32"),
            ("-128i8 as i64", -128, "i64"),
            ("4294967295u32 as i64", 4294967295, "i64"),
            ("(255u8 as i8) as i64", -1, "i64"),
            ("-9223372036854775808i64 as u32", 0, "u32"),
            ("0xffffu16 as i16", -1, "i16"),
            ("127 as i8", 127, "i8"),
            ("2147483648 as i64", 2147483648, "i64"),
            ("4294967295 as u32", 4294967295, "u32"),
            ("-128 as i8", -128, "i8"),
        ] {
            let expr = syn::parse_str::<syn::ExprCast>(source).unwrap();
            let Expression::IntLiteral { value, meta } = lower_cast(&expr, Meta::new()).unwrap()
            else {
                panic!("cast did not lower to a literal: {source}");
            };
            assert_eq!(value, expected, "{source}");
            assert_eq!(meta["rust_numeric_suffix"], suffix, "{source}");
        }
        for (source, diagnostic) in [
            ("2147483648 as u8", "outside `u8`"),
            ("255 as i8", "outside `i8`"),
            ("-1 as u8", "unsigned Rust literal"),
        ] {
            let cast = syn::parse_str::<syn::ExprCast>(source).unwrap();
            assert!(
                lower_cast(&cast, Meta::new())
                    .unwrap_err()
                    .to_string()
                    .contains(diagnostic)
            );
        }
    }

    #[test]
    fn literal_bounds_include_signed_minimum() {
        for (source, expected) in [
            ("-128i8", -128),
            ("-32768i16", -32768),
            ("-2147483648i32", -2147483648),
            ("-9223372036854775808i64", i64::MIN),
            ("9223372036854775807i64", i64::MAX),
            ("4294967295u32", 4294967295),
        ] {
            let expr = syn::parse_str(source).unwrap();
            let context = crush_walker_core_no_tree_sitter::LowerCtx::new(source, "test", "rust");
            let Expression::IntLiteral { value, .. } =
                crate::lower_expr::lower_expr(&expr, &context).unwrap()
            else {
                panic!("expected literal: {source}");
            };
            assert_eq!(value, expected, "{source}");
        }
        for source in [
            "128i8", "256u8", "-129i8", "-1u8", "1u64", "1i128", "1u128", "1usize", "1isize",
            "1.5f32",
        ] {
            let source = format!("fn main() {{ let x = {source}; }}");
            let error = crate::rust_to_cast(&source).unwrap_err().to_string();
            assert!(
                error.contains("CRUSH-xps-8") && error.contains("at 1:"),
                "{error}"
            );
        }
    }

    #[test]
    fn unproven_casts_fail_at_the_source_boundary() {
        for (source, diagnostic) in [
            (
                "fn main() { let n = 300; println!(\"{}\", n as u8); }",
                "nonliteral Rust cast operand",
            ),
            (
                "fn value() -> i64 { 300 } fn main() { println!(\"{}\", value() as u8); }",
                "nonliteral Rust cast operand",
            ),
            (
                "fn main() { let x = (1 + 2) as u8; }",
                "nonliteral Rust cast operand",
            ),
            (
                "fn main() { let x = 1.5 as i32; }",
                "noninteger Rust cast operand",
            ),
            (
                "fn main() { let x = 'a' as u8; }",
                "noninteger Rust cast operand",
            ),
            (
                "fn main() { let x = 1i64 as f64; }",
                "unsupported Rust integer type",
            ),
            (
                "fn main() { let x = 1i64 as u64; }",
                "unsupported Rust integer type",
            ),
            (
                "fn main() { let x = 1i64 as usize; }",
                "unsupported Rust integer type",
            ),
            (
                "fn main() { let x = 1i64 as isize; }",
                "unsupported Rust integer type",
            ),
            (
                "fn main() { let x = 1i64 as std::primitive::u8; }",
                "unsupported Rust cast target",
            ),
        ] {
            let error = crate::rust_to_cast(source).unwrap_err().to_string();
            assert!(
                error.contains(diagnostic)
                    && error.contains("at 1:")
                    && error.contains("CRUSH-xps-8"),
                "{source}: {error}"
            );
        }
    }

    #[test]
    fn source_hints_survive_binding_and_value_passes() {
        let program = crate::rust_to_cast("fn id(x: i16) -> i16 { x } fn main() { let a: i16 = 12; let b = 12u8; let c = |x: i32| -> i32 { x }; }").unwrap();
        let id = &program.functions["id"];
        assert_eq!(id.meta["rust_numeric_params"], json!(["i16"]));
        assert_eq!(id.meta["rust_numeric_return"], "i16");
        let body = &program.functions["main"].body;
        let crush_cast::Statement::VarDecl {
            value: Expression::IntLiteral { meta: literal, .. },
            meta: local,
            ..
        } = &body[0]
        else {
            panic!("expected declaration");
        };
        assert_eq!(local["rust_numeric_hint"], "i16");
        assert!(local.contains_key("rust_binding"));
        assert_eq!(literal["rust_numeric_suffix"], "");
        let crush_cast::Statement::VarDecl {
            value: Expression::IntLiteral { meta, .. },
            ..
        } = &body[1]
        else {
            panic!("expected literal");
        };
        assert_eq!(meta["rust_numeric_suffix"], "u8");
        let crush_cast::Statement::VarDecl {
            value: Expression::Lambda { meta, .. },
            ..
        } = &body[2]
        else {
            panic!("expected closure");
        };
        assert_eq!(meta["rust_numeric_params"], json!(["i32"]));
        assert_eq!(meta["rust_numeric_return"], "i32");
    }
}
