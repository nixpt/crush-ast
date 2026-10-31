//! JavaScript value semantics: one implementation for every crush backend.
//!
//! The JS walkers lower the JS operators whose meaning differs from crush's
//! to `js.*` capabilities (`js.add`, `js.truthy`, `js.loose_eq`,
//! `js.strict_eq`, `js.format`).
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
//! a guess: object-to-primitive conversion, circular structures in
//! `console.log`, column-grouping arrays of non-ASCII text.

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

const DEPTH: usize = 2;
const BREAK_LENGTH: usize = 80;
const COMPACT: usize = 3;
const MAX_ARRAY_LENGTH: usize = 100;
const MAX_STRING_LENGTH: usize = 10_000;
const MIN_LINE_LENGTH: usize = 16;

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
        // Null only equals null (JS `== null` is the nullish test,
        // CRUSH-289): any other value — a function included — is
        // simply unequal, never an error.
        (Null, Null) => true,
        (Null, _) | (_, Null) => false,
        (Other(t), _) | (_, Other(t)) => return Err(other_type(t)),
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

/// JS `===` (IsStrictlyEqual): no coercion, so different types are `false`
/// (`1 === '1'`, `true === 1`, `null === 0`) rather than a number conversion;
/// there is one number type (`1 === 1.0`, and `NaN` equals nothing);
/// `null === null`; two objects are equal only when they are the same object.
pub fn strict_eq(a: &JsNode, b: &JsNode) -> Result<bool, String> {
    use JsNode::*;
    Ok(match (a, b) {
        (Other(t), _) | (_, Other(t)) => return Err(other_type(t)),
        (Null, Null) => true,
        (Null, _) | (_, Null) => false,
        (Array { id: x, .. }, Array { id: y, .. }) | (Object { id: x, .. }, Object { id: y, .. }) => x == y,
        // A container against anything else (including the other container
        // kind) is a different type (or a different object): never equal.
        (Array { .. } | Object { .. }, _) | (_, Array { .. } | Object { .. }) => false,
        (Str(x), Str(y)) => x == y,
        (Bool(x), Bool(y)) => x == y,
        (Int(x), Int(y)) => x == y,
        (Int(x), Float(y)) => *x as f64 == *y,
        (Float(x), Int(y)) => *x == *y as f64,
        (Float(x), Float(y)) => x == y,
        // Different primitive types: `'1' === 1`, `true === 1`.
        _ => false,
    })
}

/// `console.log(...args)`'s line (node's `util.format`): strings as they
/// are, everything else as `util.inspect` shows it, separated by spaces.
/// `%` format directives in a leading string are loud.
pub fn format(args: &[JsNode]) -> Result<String, String> {
    if let Some(JsNode::Str(first)) = args.first() {
        if args.len() == 1 {
            return Ok(first.to_string());
        }
        let b = first.as_bytes();
        if b.windows(2).any(|w| w[0] == b'%' && b"sjdOoifc%".contains(&w[1])) {
            return Err("console.log format directives (`%s`, `%d`, `%o`, `%%`, …) are not supported by the JS runtime yet".to_string());
        }
    }
    let mut out = String::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        match a {
            JsNode::Str(s) => out.push_str(s),
            other => out.push_str(&inspect(other)?),
        }
    }
    Ok(out)
}

/// node's `util.inspect(value)` with the default options.
pub fn inspect(v: &JsNode) -> Result<String, String> {
    let mut ctx = Ctx { indentation: 0, current_depth: 0, seen: Vec::new() };
    format_value(&mut ctx, v, 0)
}

struct Ctx {
    indentation: usize,
    current_depth: usize,
    seen: Vec<usize>,
}

/// JS `.length`: UTF-16 code units.
fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn format_value(ctx: &mut Ctx, v: &JsNode, recurse: usize) -> Result<String, String> {
    match v {
        JsNode::Null => Ok("null".to_string()),
        JsNode::Bool(b) => Ok(b.to_string()),
        JsNode::Int(i) => Ok(int_text(*i)),
        JsNode::Float(f) if *f == 0.0 && f.is_sign_negative() => Ok("-0".to_string()),
        JsNode::Float(f) => Ok(number_text(*f)),
        JsNode::Str(s) => format_string(ctx, s),
        JsNode::Other(t) => Err(format!("console.log of a crush {t} value is not supported by the JS runtime")),
        JsNode::Array { id, len, items } => {
            if ctx.seen.contains(id) {
                return Err("console.log of a circular structure is not supported by the JS runtime yet".to_string());
            }
            if *len == 0 {
                return Ok("[]".to_string());
            }
            if recurse > DEPTH {
                return Ok("[Array]".to_string());
            }
            let items = items.as_ref().ok_or("internal error: array not expanded for console.log")?;
            let recurse = recurse + 1;
            ctx.seen.push(*id);
            ctx.current_depth = recurse;
            let shown = (*len).min(MAX_ARRAY_LENGTH);
            let mut output = Vec::with_capacity(shown + 1);
            for item in items.iter().take(shown) {
                ctx.indentation += 2;
                let s = format_value(ctx, item, recurse);
                ctx.indentation -= 2;
                output.push(s?);
            }
            if *len > shown {
                let n = len - shown;
                output.push(format!("... {n} more item{}", if n > 1 { "s" } else { "" }));
            }
            ctx.seen.pop();
            reduce_to_single_string(ctx, output, ("[", "]"), Some(items), recurse)
        }
        JsNode::Object { id, len, entries } => {
            if ctx.seen.contains(id) {
                return Err("console.log of a circular structure is not supported by the JS runtime yet".to_string());
            }
            if *len == 0 {
                return Ok("{}".to_string());
            }
            if recurse > DEPTH {
                return Ok("[Object]".to_string());
            }
            let entries = entries.as_ref().ok_or("internal error: object not expanded for console.log")?;
            let recurse = recurse + 1;
            ctx.seen.push(*id);
            ctx.current_depth = recurse;
            let mut output = Vec::with_capacity(entries.len());
            for (key, value) in property_order(entries) {
                ctx.indentation += 2;
                let s = format_value(ctx, value, recurse);
                ctx.indentation -= 2;
                output.push(format!("{}: {}", key_text(key), s?));
            }
            ctx.seen.pop();
            reduce_to_single_string(ctx, output, ("{", "}"), None, recurse)
        }
    }
}

/// JS property order: array-index keys (`'0'`, `'7'`) first in numeric
/// order, then the rest in insertion order.
fn property_order<'b, 'a>(entries: &'b [(String, JsNode<'a>)]) -> Vec<&'b (String, JsNode<'a>)> {
    let index = |k: &str| -> Option<u64> {
        let canonical = k == "0" || (!k.is_empty() && !k.starts_with('0') && k.bytes().all(|c| c.is_ascii_digit()));
        if !canonical || k.len() > 10 {
            return None;
        }
        k.parse::<u64>().ok().filter(|n| *n < u32::MAX as u64)
    };
    let mut indexed: Vec<(u64, &(String, JsNode))> = entries.iter().filter_map(|e| index(&e.0).map(|n| (n, e))).collect();
    indexed.sort_by_key(|(n, _)| *n);
    let mut out: Vec<&(String, JsNode)> = indexed.into_iter().map(|(_, e)| e).collect();
    out.extend(entries.iter().filter(|e| index(&e.0).is_none()));
    out
}

fn key_text(key: &str) -> String {
    let mut chars = key.chars();
    let identifier = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !identifier {
        str_escape(key)
    } else if key == "__proto__" {
        "['__proto__']".to_string()
    } else {
        key.to_string()
    }
}

fn format_string(ctx: &Ctx, s: &str) -> Result<String, String> {
    let mut s = Cow::Borrowed(s);
    let mut trailer = String::new();
    let len = js_len(&s);
    if len > MAX_STRING_LENGTH {
        if !s.is_ascii() {
            return Err("console.log of a non-ASCII string over 10000 characters inside an array or object is not supported yet".to_string());
        }
        let n = len - MAX_STRING_LENGTH;
        trailer = format!("... {n} more character{}", if n > 1 { "s" } else { "" });
        s = Cow::Owned(s[..MAX_STRING_LENGTH].to_string());
    }
    let len = js_len(&s);
    if len > MIN_LINE_LENGTH && len + ctx.indentation + 4 > BREAK_LENGTH {
        let sep = format!(" +\n{}", " ".repeat(ctx.indentation + 2));
        let lines: Vec<String> = s.split_inclusive('\n').map(str_escape).collect();
        return Ok(lines.join(&sep) + &trailer);
    }
    Ok(str_escape(&s) + &trailer)
}

/// node's `strEscape`: single quotes unless the text has one (then `"`, or
/// a backtick when it has both), control characters as `\n` / `\x1B`.
fn str_escape(s: &str) -> String {
    let mut quote = '\'';
    if s.contains('\'') {
        if !s.contains('"') {
            quote = '"';
        } else if !s.contains('`') && !s.contains("${") {
            quote = '`';
        }
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        let cp = c as u32;
        if (c == '\'' && quote == '\'') || c == '\\' || cp < 32 || (127..160).contains(&cp) {
            match cp {
                8 => out.push_str("\\b"),
                9 => out.push_str("\\t"),
                10 => out.push_str("\\n"),
                12 => out.push_str("\\f"),
                13 => out.push_str("\\r"),
                39 => out.push_str("\\'"),
                92 => out.push_str("\\\\"),
                _ => out.push_str(&format!("\\x{cp:02X}")),
            }
        } else {
            out.push(c);
        }
    }
    out.push(quote);
    out
}

fn reduce_to_single_string(
    ctx: &Ctx,
    output: Vec<String>,
    braces: (&str, &str),
    items: Option<&Vec<JsNode>>,
    recurse: usize,
) -> Result<String, String> {
    let entries = output.len();
    let mut output = output;
    if let Some(items) = items.filter(|_| entries > 6) {
        output = group_array_elements(ctx, output, items)?;
    }
    if ctx.current_depth - recurse < COMPACT && entries == output.len() {
        let start = output.len() + ctx.indentation + braces.0.len() + 10;
        if is_below_break_length(&output, start) {
            let joined = output.join(", ");
            if !joined.contains('\n') {
                return Ok(format!("{} {joined} {}", braces.0, braces.1));
            }
        }
    }
    let indentation = format!("\n{}", " ".repeat(ctx.indentation));
    Ok(format!("{}{indentation}  {}{indentation}{}", braces.0, output.join(&format!(",{indentation}  ")), braces.1))
}

fn is_below_break_length(output: &[String], start: usize) -> bool {
    let mut total = output.len() + start;
    if total + output.len() > BREAK_LENGTH {
        return false;
    }
    for o in output {
        total += js_len(o);
        if total > BREAK_LENGTH {
            return false;
        }
    }
    true
}

/// Display width of an inspected entry (node's `getStringWidth`). node
/// measures non-ASCII text with ICU's East Asian width tables; only the
/// ranges checked against node to be one column wide are known here
/// (Latin-1/Extended, Greek, Cyrillic, dashes and quotes, currency signs).
/// Anything else is loud.
fn display_width(s: &str) -> Result<usize, String> {
    let mut width = 0;
    for c in s.chars() {
        width += match c as u32 {
            0..=31 => 0,
            32..=126 => 1,
            0xa0..=0x24f | 0x370..=0x3ff | 0x400..=0x482 | 0x48a..=0x52f | 0x2010..=0x2027 | 0x20a0..=0x20bf => 1,
            _ => return Err(format!(
                "console.log of an array of more than 6 entries containing {c:?} is not supported by the JS runtime yet (node measures its display width with ICU)"
            )),
        };
    }
    Ok(width)
}

fn pad(s: &str, width: usize, start: bool) -> String {
    let fill = " ".repeat(width.saturating_sub(js_len(s)));
    if start { fill + s } else { s.to_string() + &fill }
}

/// node's `groupArrayElements`: arrays of more than six short entries are
/// laid out in aligned columns.
fn group_array_elements(ctx: &Ctx, output: Vec<String>, items: &[JsNode]) -> Result<Vec<String>, String> {
    let mut total_length = 0;
    let mut max_length = 0;
    let output_length = if output.len() > MAX_ARRAY_LENGTH { output.len() - 1 } else { output.len() };
    let separator_space = 2;
    let mut data_len = Vec::with_capacity(output_length);
    for o in output.iter().take(output_length) {
        let len = display_width(o)?;
        data_len.push(len);
        total_length += len + separator_space;
        max_length = max_length.max(len);
    }
    let actual_max = max_length + separator_space;
    if actual_max * 3 + ctx.indentation < BREAK_LENGTH
        && (total_length as f64 / actual_max as f64 > 5.0 || max_length <= 6)
    {
        let approx_char_heights = 2.5;
        let average_bias = (actual_max as f64 - total_length as f64 / output.len() as f64).sqrt();
        let biased_max = (actual_max as f64 - 3.0 - average_bias).max(1.0);
        let columns = ((approx_char_heights * biased_max * output_length as f64).sqrt() / biased_max + 0.5).floor()
            .min(((BREAK_LENGTH - ctx.indentation) / actual_max) as f64)
            .min((COMPACT * 4) as f64)
            .min(15.0);
        if columns <= 1.0 {
            return Ok(output);
        }
        let columns = columns as usize;
        let mut max_line_length = Vec::with_capacity(columns);
        for i in 0..columns {
            let mut line = 0;
            let mut j = i;
            while j < data_len.len() {
                line = line.max(data_len[j]);
                j += columns;
            }
            max_line_length.push(line + separator_space);
        }
        let numbers = items.iter().take(output.len()).all(|v| matches!(v, JsNode::Int(_) | JsNode::Float(_)));
        let mut tmp = Vec::new();
        let mut i = 0;
        while i < output_length {
            let max = (i + columns).min(output_length);
            let mut line = String::new();
            let mut j = i;
            while j < max - 1 {
                let padding = max_line_length[j - i] + js_len(&output[j]) - data_len[j];
                line.push_str(&pad(&format!("{}, ", output[j]), padding, numbers));
                j += 1;
            }
            if numbers {
                let padding = max_line_length[j - i] + js_len(&output[j]) - data_len[j] - separator_space;
                line.push_str(&pad(&output[j], padding, true));
            } else {
                line.push_str(&output[j]);
            }
            tmp.push(line);
            i += columns;
        }
        if output.len() > MAX_ARRAY_LENGTH {
            tmp.push(output[output_length].clone());
        }
        return Ok(tmp);
    }
    Ok(output)
}
