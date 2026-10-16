//! Shared, std-only method semantics. Embedded verbatim in both AOT backends.
#[derive(Clone, Copy, Debug)]
pub enum Input<'a> {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(&'a str),
    Invalid,
}
#[derive(Debug, PartialEq)]
pub enum Output {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
}
fn text<'a>(args: &[Input<'a>], n: usize) -> Result<&'a str, String> {
    match args.get(n) {
        Some(Input::Text(s)) => Ok(s),
        _ => Err(format!("expected string at index {n}")),
    }
}
fn integer(args: &[Input<'_>], n: usize) -> Result<i64, String> {
    match args.get(n) {
        Some(Input::Int(i)) => Ok(*i),
        _ => Err(format!("expected integer at index {n}")),
    }
}
fn float(args: &[Input<'_>], n: usize, strict: bool) -> Result<f64, String> {
    match args.get(n) {
        Some(Input::Float(f)) => Ok(*f),
        Some(Input::Int(i)) if !strict => Ok(*i as f64),
        Some(Input::Text(s)) if !strict => s
            .parse()
            .map_err(|_| format!("expected number at index {n}")),
        _ => Err(format!("expected number at index {n}")),
    }
}
pub fn arity(name: &str) -> Option<usize> {
    Some(match name {
        "str.to_lower" | "str.to_upper" | "str.trim" | "math.abs" | "math.sqrt" | "math.floor"
        | "math.ceil" | "math.round" => 1,
        "str.contains"
        | "str.starts_with"
        | "str.ends_with"
        | "rust.str.starts_with"
        | "rust.str.ends_with"
        | "math.pow" => 2,
        "str.replace" => 3,
        "rust.num" => 4,
        _ => return None,
    })
}
pub fn call(name: &str, args: &[Input<'_>]) -> Result<Output, String> {
    let argc = arity(name).ok_or_else(|| format!("unknown method capability {name}"))?;
    if args.len() != argc {
        return Err(format!(
            "{name}: expected {argc} arguments, got {}",
            args.len()
        ));
    }
    if name == "rust.num" {
        return numeric(args);
    }
    if name.starts_with("math.") {
        let a = float(args, 0, false)?;
        return Ok(Output::Float(match name {
            "math.abs" => a.abs(),
            "math.sqrt" => a.sqrt(),
            "math.floor" => a.floor(),
            "math.ceil" => a.ceil(),
            "math.round" => a.round(),
            "math.pow" => a.powf(float(args, 1, false)?),
            _ => unreachable!(),
        }));
    }
    let a = text(args, 0)?;
    Ok(match name {
        "str.to_lower" => Output::Text(a.to_lowercase()),
        "str.to_upper" => Output::Text(a.to_uppercase()),
        "str.trim" => Output::Text(a.trim().to_owned()),
        "str.replace" => Output::Text(a.replace(text(args, 1)?, text(args, 2)?)),
        "str.contains" => Output::Bool(a.contains(text(args, 1)?)),
        "str.starts_with" => Output::Int(i64::from(a.starts_with(text(args, 1)?))),
        "str.ends_with" => Output::Int(i64::from(a.ends_with(text(args, 1)?))),
        "rust.str.starts_with" => Output::Bool(a.starts_with(text(args, 1)?)),
        "rust.str.ends_with" => Output::Bool(a.ends_with(text(args, 1)?)),
        _ => unreachable!(),
    })
}
fn numeric(args: &[Input<'_>]) -> Result<Output, String> {
    let method = text(args, 0)?;
    let ty = text(args, 1)?;
    if matches!(method, "abs" | "sqrt" | "floor" | "ceil" | "round")
        && !matches!(args[3], Input::Null)
    {
        return Err("unary numeric method requires null exponent".into());
    }
    if ty == "f64" {
        let a = float(args, 2, true)?;
        return Ok(Output::Float(match method {
            "abs" => a.abs(),
            "sqrt" => a.sqrt(),
            "floor" => a.floor(),
            "ceil" => a.ceil(),
            "round" => a.round(),
            "powf" => a.powf(float(args, 3, true)?),
            "powi" => {
                a.powi(i32::try_from(integer(args, 3)?).map_err(|_| "powi exponent outside i32")?)
            }
            _ => return Err("unsupported f64 method".into()),
        }));
    }
    let (bits, signed) = match ty {
        "i8" => (8, true),
        "i16" => (16, true),
        "i32" => (32, true),
        "i64" => (64, true),
        "u8" => (8, false),
        "u16" => (16, false),
        "u32" => (32, false),
        "u64" | "usize" => (64, false),
        _ => return Err("unsupported numeric descriptor".into()),
    };
    let a = integer(args, 2)?;
    let in_range = if signed {
        bits == 64 || a >= -(1i64 << (bits - 1)) && a < (1i64 << (bits - 1))
    } else {
        a >= 0 && (bits == 64 || (a as u64) < (1u64 << bits))
    };
    if !in_range {
        return Err("noncanonical integer receiver".into());
    }
    let mask = u64::MAX >> (64 - bits);
    let raw = match method {
        "abs" if signed => a.wrapping_abs() as u64,
        "pow" => {
            let mut e = u32::try_from(integer(args, 3)?).map_err(|_| "pow exponent outside u32")?;
            let mut base = a as u64;
            let mut out = 1u64;
            while e != 0 {
                if e & 1 != 0 {
                    out = out.wrapping_mul(base) & mask;
                }
                e >>= 1;
                base = base.wrapping_mul(base) & mask;
            }
            out
        }
        _ => return Err("unsupported integer method".into()),
    } & mask;
    let result = if signed && bits < 64 && raw >= (1u64 << (bits - 1)) {
        (raw | !mask) as i64
    } else if signed {
        raw as i64
    } else {
        i64::try_from(raw).map_err(|_| "unsigned result outside Crush Int")?
    };
    if ty == "usize" && !(0..=65535).contains(&result) {
        return Err("usize result outside portable proof bound".into());
    }
    Ok(Output::Int(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_patterns_and_boolean_contracts() {
        assert_eq!(
            call("str.to_upper", &[Input::Text("Straße")]),
            Ok(Output::Text("STRASSE".into()))
        );
        assert_eq!(
            call("str.to_lower", &[Input::Text("ΟΣ")]),
            Ok(Output::Text("ος".into()))
        );
        assert_eq!(
            call("str.trim", &[Input::Text("\u{a0}λ\u{2003}")]),
            Ok(Output::Text("λ".into()))
        );
        assert_eq!(
            call(
                "str.replace",
                &[Input::Text("a"), Input::Text(""), Input::Text("λ")]
            ),
            Ok(Output::Text("λaλ".into()))
        );
        assert_eq!(
            call("str.contains", &[Input::Text("a\0b"), Input::Text("\0")]),
            Ok(Output::Bool(true))
        );
        assert_eq!(
            call("str.starts_with", &[Input::Text("a"), Input::Text("")]),
            Ok(Output::Int(1))
        );
        assert_eq!(
            call("rust.str.ends_with", &[Input::Text("a"), Input::Text("")]),
            Ok(Output::Bool(true))
        );
    }
    #[test]
    fn integer_widths_wrap_and_float_operations_stay_distinct() {
        for (op, ty, a, b, out) in [
            ("pow", "i8", 6, 3, -40),
            ("pow", "u8", 20, 2, 144),
            ("abs", "i8", -128, 0, -128),
            ("pow", "i64", 2, 3, 8),
            ("pow", "usize", 2, 3, 8),
        ] {
            assert_eq!(
                call(
                    "rust.num",
                    &[
                        Input::Text(op),
                        Input::Text(ty),
                        Input::Int(a),
                        if op == "abs" {
                            Input::Null
                        } else {
                            Input::Int(b)
                        }
                    ]
                ),
                Ok(Output::Int(out))
            );
        }
        assert_eq!(
            call(
                "rust.num",
                &[
                    Input::Text("powi"),
                    Input::Text("f64"),
                    Input::Float(2.0),
                    Input::Int(-2)
                ]
            ),
            Ok(Output::Float(0.25))
        );
        assert!(
            call(
                "rust.num",
                &[
                    Input::Text("powi"),
                    Input::Text("f64"),
                    Input::Float(2.0),
                    Input::Float(2.0)
                ]
            )
            .is_err()
        );
        assert!(
            call(
                "rust.num",
                &[
                    Input::Text("pow"),
                    Input::Text("u64"),
                    Input::Int(2),
                    Input::Int(63)
                ]
            )
            .is_err()
        );
        assert!(
            call(
                "rust.num",
                &[
                    Input::Text("pow"),
                    Input::Text("i8"),
                    Input::Int(128),
                    Input::Int(2)
                ]
            )
            .is_err()
        );
        assert!(
            call(
                "rust.num",
                &[
                    Input::Text("pow"),
                    Input::Text("i128"),
                    Input::Int(1),
                    Input::Int(2)
                ]
            )
            .is_err()
        );
        assert!(call("str.replace", &[]).is_err());
        assert!(call("unknown", &[]).is_err());
        assert!(call("str.trim", &[Input::Int(2)]).is_err());
    }
}
