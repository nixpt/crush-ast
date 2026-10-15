//! JavaScript value semantics: one implementation for every crush backend.
//!
//! The JS walkers lower the JS operators whose meaning differs from crush's
//! to `js.*` capabilities (`js.add`, `js.truthy`, `js.loose_eq`).
//! Crush's own `+`, `&&`, `||` and `==` are unchanged: other languages share
//! them.
//!
//! This file uses `std` only and names nothing outside itself. crush-vm
//! compiles it as a module, and the AOT Rust runtime embeds its text verbatim
//! (`crush_vm::js_ops::JS_CORE_SRC`), so the VM and AOT Rust run the same
//! code. Each runtime only converts its own values to a [`JsNode`]. The AOT C
//! runtime mirrors this file (`crush-aot/src/js_runtime.c`).
//!
//! What JS can do but this can't yet is an `Err` (a loud runtime error), never
//! a guess: object-to-primitive conversion, for one.

use std::borrow::Cow;

/// A crush value seen as a JavaScript value.
#[derive(Debug, Clone)]
pub enum JsNode<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Cow<'a, str>),
    /// `id` is the array's identity. `items` holds at most [`ITEMS`] elements
    /// and is `None` when the converter didn't expand this level.
    Array { id: usize, len: usize, items: Option<Vec<JsNode<'a>>> },
    /// A plain object; `entries` in insertion order, `None` when not expanded.
    Object { id: usize, len: usize, entries: Option<Vec<(String, JsNode<'a>)>> },
    /// A crush value with no JavaScript meaning (its type name).
    Other(&'static str),
}

/// How many container levels a converter expands for `console.log`: node's
/// `util.inspect` default depth is 2, and level 3 only needs its length.
pub const EXPAND: usize = 3;
/// Array elements a converter keeps for `console.log`: node prints 100 and
/// looks at the 101st when choosing column alignment.
pub const ITEMS: usize = 101;

fn not_primitive(what: &str) -> String {
    format!("{what} with an array or object operand is not supported by the JS runtime yet (needs ToPrimitive)")
}

fn other_type(t: &str) -> String {
    format!("a crush {t} value has no JavaScript meaning")
}

/// `Number.prototype.toString()`: `6`, `0.30000000000000004`, `1e+21`,
/// `1e-7`; `-0` is `0`.
pub fn number_text(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if f == 0.0 {
        return "0".to_string();
    }
    // The fewest digits that read back as `f` (Rust's `{:e}`), then — as the
    // spec asks — the closest such digit string, ties to even: Rust's
    // shortest form breaks an exact tie upwards (`11.261367797851563` for
    // 11.2613677978515625, where JS prints `…562`), while its fixed-precision
    // `{:.Ne}` rounds the exact value half-to-even.
    let shortest = format!("{:e}", f.abs());
    let k = shortest.split_once('e').map_or(1, |(m, _)| m.chars().filter(char::is_ascii_digit).count());
    let exact = format!("{:.*e}", k - 1, f.abs());
    let chosen = if exact.parse::<f64>() == Ok(f.abs()) { exact } else { shortest };
    let (mant, exp) = chosen.split_once('e').unwrap_or((&chosen, "0"));
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let k = digits.len() as i64;
    // Number::toString: the value is 0.digits × 10^n.
    let n = exp.parse::<i64>().unwrap_or(0) + 1;
    let sign = if f < 0.0 { "-" } else { "" };
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let rest = if k > 1 { format!(".{}", &digits[1..]) } else { String::new() };
        format!("{}{rest}e{}{}", &digits[..1], if e < 0 { '-' } else { '+' }, e.abs())
    };
    format!("{sign}{body}")
}

/// An int as JS prints it. JS numbers are doubles, so past 2^53 the text is
/// the nearest double's (`7774233818054199000`, not `…199296`).
pub fn int_text(i: i64) -> String {
    if i.unsigned_abs() <= 1 << 53 { i.to_string() } else { number_text(i as f64) }
}

/// JS `ToString` of a primitive.
pub fn to_text<'b>(v: &'b JsNode, what: &str) -> Result<Cow<'b, str>, String> {
    Ok(match v {
        JsNode::Null => Cow::Borrowed("null"),
        JsNode::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
        JsNode::Int(i) => Cow::Owned(int_text(*i)),
        JsNode::Float(f) => Cow::Owned(number_text(*f)),
        JsNode::Str(s) => Cow::Borrowed(s.as_ref()),
        JsNode::Array { .. } | JsNode::Object { .. } => return Err(not_primitive(what)),
        JsNode::Other(t) => return Err(other_type(t)),
    })
}

fn is_js_space(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}'
        | '\u{3000}' | '\u{feff}')
}

/// JS `ToNumber` of a string (`StringToNumber`): surrounding whitespace is
/// ignored, `''` is 0, `0x`/`0o`/`0b` prefixes, `Infinity`, decimal literals;
/// anything else is NaN.
pub fn string_to_number(s: &str) -> Result<f64, String> {
    let t = s.trim_matches(is_js_space);
    if t.is_empty() {
        return Ok(0.0);
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = t.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return Ok(f64::NAN);
            }
            return match u128::from_str_radix(digits, radix) {
                // `as` rounds to the nearest double, like JS.
                Ok(n) => Ok(n as f64),
                Err(_) => Err(format!("converting the {}-digit literal {prefix}… to a number is not supported yet", digits.len())),
            };
        }
    }
    match t {
        "Infinity" | "+Infinity" => return Ok(f64::INFINITY),
        "-Infinity" => return Ok(f64::NEG_INFINITY),
        _ => {}
    }
    // StrDecimalLiteral: [+-] (digits [. digits] | . digits) [(e|E) [+-] digits]
    let b = t.as_bytes();
    let mut i = 0;
    if matches!(b[0], b'+' | b'-') {
        i = 1;
    }
    let digits = |i: &mut usize| {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    };
    let mut mantissa = digits(&mut i);
    if i < b.len() && b[i] == b'.' {
        i += 1;
        mantissa += digits(&mut i);
    }
    if mantissa == 0 {
        return Ok(f64::NAN);
    }
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        i += 1;
        if i < b.len() && matches!(b[i], b'+' | b'-') {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return Ok(f64::NAN);
        }
    }
    if i != b.len() {
        return Ok(f64::NAN);
    }
    Ok(t.parse::<f64>().unwrap_or(f64::NAN))
}

/// JS `+`. A string on either side concatenates the `ToString` of both
/// (`'x' + 6` is `x6`); otherwise numeric addition of the `ToNumber`s
/// (`true + 1` is 2, `null + 1` is 1). Two ints stay an int (overflow is a
/// loud error, as for crush's `+`).
pub fn add(a: &JsNode, b: &JsNode) -> Result<JsNode<'static>, String> {
    for v in [a, b] {
        match v {
            JsNode::Array { .. } | JsNode::Object { .. } => return Err(not_primitive("JS `+`")),
            JsNode::Other(t) => return Err(other_type(t)),
            _ => {}
        }
    }
    if matches!(a, JsNode::Str(_)) || matches!(b, JsNode::Str(_)) {
        let mut s = to_text(a, "JS `+`")?.into_owned();
        s.push_str(&to_text(b, "JS `+`")?);
        return Ok(JsNode::Str(Cow::Owned(s)));
    }
    let int = |v: &JsNode| match v {
        JsNode::Null => Some(0),
        JsNode::Bool(b) => Some(*b as i64),
        JsNode::Int(i) => Some(*i),
        _ => None,
    };
    if let (Some(x), Some(y)) = (int(a), int(b)) {
        return x.checked_add(y).map(JsNode::Int).ok_or_else(|| "arithmetic overflow".to_string());
    }
    Ok(JsNode::Float(primitive_number(a)? + primitive_number(b)?))
}

/// JS `ToNumber` of a primitive.
fn primitive_number(v: &JsNode) -> Result<f64, String> {
    Ok(match v {
        JsNode::Null => 0.0,
        JsNode::Bool(b) => *b as i64 as f64,
        JsNode::Int(i) => *i as f64,
        JsNode::Float(f) => *f,
        JsNode::Str(s) => string_to_number(s)?,
        JsNode::Array { .. } | JsNode::Object { .. } => return Err(not_primitive("JS number conversion")),
        JsNode::Other(t) => return Err(other_type(t)),
    })
}

/// JS truthiness: `0`, `NaN`, `''`, `null` and `false` are falsy; arrays and
/// objects (even empty ones) are truthy.
pub fn truthy(v: &JsNode) -> bool {
    match v {
        JsNode::Null => false,
        JsNode::Bool(b) => *b,
        JsNode::Int(i) => *i != 0,
        JsNode::Float(f) => !(*f == 0.0 || f.is_nan()),
        JsNode::Str(s) => !s.is_empty(),
        JsNode::Array { .. } | JsNode::Object { .. } | JsNode::Other(_) => true,
    }
}

/// JS `==` (IsLooselyEqual) for primitives: `'1' == 1`, `true == 1`,
/// `null == null`; NaN equals nothing; two objects are equal only when they
/// are the same object. Object against primitive needs ToPrimitive: loud.
/// (`undefined` has no crush value yet, so `null == undefined` can't arise.)
pub fn loose_eq(a: &JsNode, b: &JsNode) -> Result<bool, String> {
    use JsNode::*;
    Ok(match (a, b) {
        (Other(t), _) | (_, Other(t)) => return Err(other_type(t)),
        (Null, Null) => true,
        (Null, _) | (_, Null) => false,
        (Array { id: x, .. }, Array { id: y, .. }) | (Object { id: x, .. }, Object { id: y, .. }) => x == y,
        (Array { .. }, Object { .. }) | (Object { .. }, Array { .. }) => false,
        (Array { .. } | Object { .. }, _) | (_, Array { .. } | Object { .. }) => {
            return Err(not_primitive("JS `==`"));
        }
        (Str(x), Str(y)) => x == y,
        (Bool(x), Bool(y)) => x == y,
        (Int(x), Int(y)) => x == y,
        (Bool(x), other) => return loose_eq(&Int(*x as i64), other),
        (other, Bool(y)) => return loose_eq(other, &Int(*y as i64)),
        (Str(s), n) | (n, Str(s)) => primitive_number(n)? == string_to_number(s)?,
        (x, y) => primitive_number(x)? == primitive_number(y)?,
    })
}
