//! Standard library capabilities for CRUSH runtime.
//!
//! Pure computation capabilities ported from exosphere's stdlib.
//! These are stdcaps — always available, no capability gate required.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// Register all standard library capabilities on the given [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps) {
    // String capabilities
    caps.register(Box::new(StrSplitCap));
    caps.register(Box::new(StrJoinCap));
    caps.register(Box::new(StrTrimCap));
    caps.register(Box::new(StrTrimStartCap));
    caps.register(Box::new(StrTrimEndCap));
    caps.register(Box::new(StrReplaceCap));
    caps.register(Box::new(StrContainsCap));
    caps.register(Box::new(StrStartsWithCap));
    caps.register(Box::new(StrEndsWithCap));
    caps.register(Box::new(StrToUpperCap));
    caps.register(Box::new(StrToLowerCap));
    caps.register(Box::new(StrPadLeftCap));
    caps.register(Box::new(StrPadRightCap));
    caps.register(Box::new(StrRepeatCap));
    caps.register(Box::new(StrSubstringCap));
    caps.register(Box::new(StrCharAtCap));
    caps.register(Box::new(StrIndexOfCap));
    caps.register(Box::new(StrFormatCap));

    // Math capabilities
    caps.register(Box::new(MathSqrtCap));
    caps.register(Box::new(MathAbsCap));
    caps.register(Box::new(MathFloorCap));
    caps.register(Box::new(MathCeilCap));
    caps.register(Box::new(MathRoundCap));
    caps.register(Box::new(MathSinCap));
    caps.register(Box::new(MathCosCap));
    caps.register(Box::new(MathTanCap));
    caps.register(Box::new(MathPowCap));
    caps.register(Box::new(MathMinCap));
    caps.register(Box::new(MathMaxCap));
    caps.register(Box::new(MathPiCap));

    // Conversion capabilities
    caps.register(Box::new(ConvToIntCap));
    caps.register(Box::new(ConvToFloatCap));
    caps.register(Box::new(ConvToStrCap));
    caps.register(Box::new(ConvToBoolCap));
    caps.register(Box::new(ConvParseIntCap));
    caps.register(Box::new(ConvParseFloatCap));
    caps.register(Box::new(ConvTypeOfCap));

    // Collections capabilities (array-only)
    caps.register(Box::new(CollLenCap));
    caps.register(Box::new(CollReverseCap));
    caps.register(Box::new(CollIncludesCap));
    caps.register(Box::new(CollFlattenCap));
    caps.register(Box::new(CollChunkCap));
    caps.register(Box::new(CollZipCap));
    caps.register(Box::new(CollUniqueCap));

    // JSON capabilities (gated by `stdlib` feature — types live behind it)
    #[cfg(feature = "stdlib")]
    {
        caps.register(Box::new(JsonParseCap));
        caps.register(Box::new(JsonStringifyCap));
        caps.register(Box::new(JsonStringifyPrettyCap));
    }

    // Path capabilities
    caps.register(Box::new(PathJoinCap));
    caps.register(Box::new(PathDirnameCap));
    caps.register(Box::new(PathBasenameCap));
    caps.register(Box::new(PathExtensionCap));
    caps.register(Box::new(PathIsAbsoluteCap));
    caps.register(Box::new(PathNormalizeCap));
    caps.register(Box::new(PathStemCap));

    // Regex capabilities (gated by `stdlib` feature — types live behind it)
    #[cfg(feature = "stdlib")]
    {
        caps.register(Box::new(RegexTestCap));
        caps.register(Box::new(RegexMatchCap));
        caps.register(Box::new(RegexFindAllCap));
        caps.register(Box::new(RegexReplaceCap));
        caps.register(Box::new(RegexSplitCap));
    }
}

fn get_str(args: &[Value], idx: usize) -> Result<String, String> {
    args.get(idx)
        // Delegate to the canonical `impl Display for Value` in
        // `crush_vm::vm::vm.rs` — replaces 14 lines of duplicated match
        // arms that previously had to be kept in lockstep with
        // `caps::value_as_text` + `vm::as_text`. `Display` is now the
        // single source of truth for how every CVM1 value surfaces as
        // a string.
        .map(|v| v.to_string())
        .ok_or_else(|| format!("missing argument at index {}", idx))
}

fn get_int(args: &[Value], idx: usize) -> Result<i64, String> {
    args.get(idx)
        .and_then(|v| match v {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(if *b { 1 } else { 0 }),
            Value::Float(f) => Some(*f as i64),
            Value::Str(s) => s.parse().ok(),
            _ => None,
        })
        .ok_or_else(|| format!("expected integer at index {}", idx))
}

fn get_float(args: &[Value], idx: usize) -> Result<f64, String> {
    args.get(idx)
        .and_then(|v| match v {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            Value::Str(s) => s.parse().ok(),
            _ => None,
        })
        .ok_or_else(|| format!("expected number at index {}", idx))
}

// Delegate to the canonical `impl Display for Value` in
// `crush_vm::vm::vm.rs` — replaces 14 lines of duplicated match arms
// that previously had to be kept in lockstep with `caps::value_as_text`
// and `vm::as_text`. The `Display` impl is now the single source of
// truth for how every CVM1 value surfaces as a string; if you find
// yourself wanting Value-specific formatting here, add it to the
// `Display` impl instead.
fn value_to_string(v: &Value) -> String {
    v.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// String Capabilities
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! str_cap {
    ($name:ident, $ic:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("str.", $ic).to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                $body(&args)
            }
        }
    };
}

str_cap!(StrSplitCap, "split", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let delim = get_str(args, 1)?;
    let parts: Vec<Value> = s.split(&delim).map(|p| Value::Str(p.to_string())).collect();
    Ok(Some(Value::new_array(parts)))
});

str_cap!(StrJoinCap, "join", 2, |args: &[Value]| {
    let arr = match &args[0] {
        Value::Array(a) => a.clone(),
        _ => return Err("str.join: first argument must be an array".to_string()),
    };
    let delim = get_str(args, 1)?;
    let strings: Result<Vec<String>, String> = arr.borrow().iter().map(|v| Ok(value_to_string(v))).collect();
    let joined = strings?.join(&delim);
    Ok(Some(Value::Str(joined)))
});

str_cap!(StrTrimCap, "trim", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    Ok(Some(Value::Str(s.trim().to_string())))
});

str_cap!(StrTrimStartCap, "trim_start", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    Ok(Some(Value::Str(s.trim_start().to_string())))
});

str_cap!(StrTrimEndCap, "trim_end", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    Ok(Some(Value::Str(s.trim_end().to_string())))
});

str_cap!(StrReplaceCap, "replace", 3, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let pattern = get_str(args, 1)?;
    let replacement = get_str(args, 2)?;
    Ok(Some(Value::Str(s.replace(&pattern, &replacement))))
});

str_cap!(StrContainsCap, "contains", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let sub = get_str(args, 1)?;
    Ok(Some(Value::Int(if s.contains(&sub) { 1 } else { 0 })))
});

str_cap!(StrStartsWithCap, "starts_with", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let prefix = get_str(args, 1)?;
    Ok(Some(Value::Int(if s.starts_with(&prefix) { 1 } else { 0 })))
});

str_cap!(StrEndsWithCap, "ends_with", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let suffix = get_str(args, 1)?;
    Ok(Some(Value::Int(if s.ends_with(&suffix) { 1 } else { 0 })))
});

str_cap!(StrToUpperCap, "to_upper", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    Ok(Some(Value::Str(s.to_uppercase())))
});

str_cap!(StrToLowerCap, "to_lower", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    Ok(Some(Value::Str(s.to_lowercase())))
});

str_cap!(StrPadLeftCap, "pad_left", 3, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let len = get_int(args, 1)? as usize;
    let ch = get_str(args, 2).unwrap_or_else(|_| " ".to_string());
    let pad_char = ch.chars().next().unwrap_or(' ');
    if s.len() >= len {
        return Ok(Some(Value::Str(s)));
    }
    let padding: String = std::iter::repeat_n(pad_char, len - s.len()).collect();
    Ok(Some(Value::Str(format!("{}{}", padding, s))))
});

str_cap!(StrPadRightCap, "pad_right", 3, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let len = get_int(args, 1)? as usize;
    let ch = get_str(args, 2).unwrap_or_else(|_| " ".to_string());
    let pad_char = ch.chars().next().unwrap_or(' ');
    if s.len() >= len {
        return Ok(Some(Value::Str(s)));
    }
    let padding: String = std::iter::repeat_n(pad_char, len - s.len()).collect();
    Ok(Some(Value::Str(format!("{}{}", s, padding))))
});

str_cap!(StrRepeatCap, "repeat", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let count = get_int(args, 1)? as usize;
    Ok(Some(Value::Str(s.repeat(count))))
});

str_cap!(StrSubstringCap, "substring", 3, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let start = get_int(args, 1)? as usize;
    let end = get_int(args, 2)? as usize;
    let end = end.min(s.len());
    let start = start.min(end);
    Ok(Some(Value::Str(s[start..end].to_string())))
});

str_cap!(StrCharAtCap, "char_at", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let idx = get_int(args, 1)? as usize;
    match s.chars().nth(idx) {
        Some(c) => Ok(Some(Value::Str(c.to_string()))),
        None => Ok(Some(Value::Str(String::new()))),
    }
});

str_cap!(StrIndexOfCap, "index_of", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let needle = get_str(args, 1)?;
    match s.find(&needle) {
        Some(pos) => Ok(Some(Value::Int(pos as i64))),
        None => Ok(Some(Value::Int(-1))),
    }
});

str_cap!(StrFormatCap, "format", 2, |args: &[Value]| {
    let template = get_str(args, 0)?;
    let mut result = template.clone();
    for arg in args.iter().skip(1) {
        if let Some(pos) = result.find("{}") {
            let replacement = value_to_string(arg);
            result = format!("{}{}{}", &result[..pos], replacement, &result[pos + 2..]);
        }
    }
    Ok(Some(Value::Str(result)))
});

// ─────────────────────────────────────────────────────────────────────────────
// Math Capabilities
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! math_unary_cap {
    ($name:ident, $ic:expr, $op:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("math.", $ic).to_string(),
                    argc: Some(1),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                let x = get_float(&args, 0)?;
                Ok(Some(Value::Float($op(x))))
            }
        }
    };
}

macro_rules! math_binary_cap {
    ($name:ident, $ic:expr, $op:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("math.", $ic).to_string(),
                    argc: Some(2),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                let a = get_float(&args, 0)?;
                let b = get_float(&args, 1)?;
                Ok(Some(Value::Float($op(a, b))))
            }
        }
    };
}

math_unary_cap!(MathSqrtCap, "sqrt", f64::sqrt);
math_unary_cap!(MathAbsCap, "abs", f64::abs);
math_unary_cap!(MathFloorCap, "floor", f64::floor);
math_unary_cap!(MathCeilCap, "ceil", f64::ceil);
math_unary_cap!(MathRoundCap, "round", f64::round);
math_unary_cap!(MathSinCap, "sin", f64::sin);
math_unary_cap!(MathCosCap, "cos", f64::cos);
math_unary_cap!(MathTanCap, "tan", f64::tan);

math_binary_cap!(MathPowCap, "pow", f64::powf);
math_binary_cap!(MathMinCap, "min", f64::min);
math_binary_cap!(MathMaxCap, "max", f64::max);

pub struct MathPiCap;
impl HostCap for MathPiCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "math.pi".to_string(),
            argc: Some(0),
            returns: true,
        }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Ok(Some(Value::Float(std::f64::consts::PI)))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversion Capabilities
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! conv_cap {
    ($name:ident, $ic:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("conv.", $ic).to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                $body(&args)
            }
        }
    };
}

conv_cap!(ConvToIntCap, "to_int", 1, |args: &[Value]| {
    let v = &args[0];
    let result = match v {
        Value::Int(i) => Value::Int(*i),
        Value::Float(f) => Value::Int(*f as i64),
        Value::Str(s) => s.trim().parse().map(Value::Int).unwrap_or(Value::Null),
        Value::Null => Value::Int(0),
        _ => Value::Null,
    };
    Ok(Some(result))
});

conv_cap!(ConvToFloatCap, "to_float", 1, |args: &[Value]| {
    let v = &args[0];
    let result = match v {
        Value::Int(i) => Value::Float(*i as f64),
        Value::Float(f) => Value::Float(*f),
        Value::Str(s) => s.trim().parse().map(Value::Float).unwrap_or(Value::Null),
        Value::Null => Value::Float(0.0),
        _ => Value::Null,
    };
    Ok(Some(result))
});

conv_cap!(ConvToStrCap, "to_str", 1, |args: &[Value]| {
    let v = &args[0];
    let result = Value::Str(value_to_string(v));
    Ok(Some(result))
});

conv_cap!(ConvToBoolCap, "to_bool", 1, |args: &[Value]| {
    let v = &args[0];
    let result = match v {
        Value::Bool(b) => Value::Bool(*b),
        Value::Int(i) => Value::Int(if *i != 0 { 1 } else { 0 }),
        Value::Float(f) => Value::Int(if *f != 0.0 { 1 } else { 0 }),
        Value::Str(s) => Value::Int(if s.is_empty() { 0 } else { 1 }),
        Value::Null => Value::Int(0),
        Value::Array(a) => Value::Int(if a.borrow().is_empty() { 0 } else { 1 }),
        Value::Map(m) => Value::Int(if m.borrow().is_empty() { 0 } else { 1 }),
        Value::Error(_) => Value::Int(1),
        Value::Bytes(b) => Value::Int(if b.is_empty() { 0 } else { 1 }),
        Value::Handle(_) => Value::Int(1),
    };
    Ok(Some(result))
});

conv_cap!(ConvParseIntCap, "parse_int", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let radix = get_int(args, 1).unwrap_or(10) as u32;
    let result = i64::from_str_radix(s.trim(), radix)
        .map(Value::Int)
        .unwrap_or(Value::Null);
    Ok(Some(result))
});

conv_cap!(ConvParseFloatCap, "parse_float", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let result = s
        .trim()
        .parse::<f64>()
        .map(Value::Float)
        .unwrap_or(Value::Null);
    Ok(Some(result))
});

conv_cap!(ConvTypeOfCap, "type_of", 1, |args: &[Value]| {
    let v = &args[0];
    let type_name = match v {
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "string",
        Value::Null => "null",
        Value::Map(_) => "map",
        Value::Array(_) => "array",
        Value::Error(_) => "error",
        Value::Bytes(_) => "bytes",
        Value::Handle(_) => "handle",
    };
    Ok(Some(Value::Str(type_name.to_string())))
});

// ─────────────────────────────────────────────────────────────────────────────
// Collections Capabilities (array-only)
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! coll_cap {
    ($name:ident, $ic:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("collections.", $ic).to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                $body(&args)
            }
        }
    };
}

coll_cap!(CollLenCap, "len", 1, |args: &[Value]| {
    let v = &args[0];
    let len = match v {
        Value::Array(a) => a.borrow().len(),
        Value::Str(s) => s.len(),
        _ => return Err("collections.len: expected array or string".to_string()),
    };
    Ok(Some(Value::Int(len as i64)))
});

coll_cap!(CollReverseCap, "reverse", 1, |args: &[Value]| {
    let v = &args[0];
    match v {
        Value::Array(a) => {
            let mut arr = a.borrow().clone();
            arr.reverse();
            Ok(Some(Value::new_array(arr)))
        }
        _ => Err("collections.reverse: expected array".to_string()),
    }
});

coll_cap!(CollIncludesCap, "includes", 2, |args: &[Value]| {
    let v = &args[0];
    let needle = &args[1];
    let found = match v {
        Value::Array(a) => a.borrow().iter().any(|x| x == needle),
        _ => return Err("collections.includes: expected array".to_string()),
    };
    Ok(Some(Value::Int(if found { 1 } else { 0 })))
});

coll_cap!(CollFlattenCap, "flatten", 1, |args: &[Value]| {
    let v = &args[0];
    match v {
        Value::Array(a) => {
            let mut flat = Vec::new();
            for item in a.borrow().iter() {
                match item {
                    Value::Array(inner) => flat.extend(inner.borrow().clone()),
                    _ => flat.push(item.clone()),
                }
            }
            Ok(Some(Value::new_array(flat)))
        }
        _ => Err("collections.flatten: expected array".to_string()),
    }
});

coll_cap!(CollChunkCap, "chunk", 2, |args: &[Value]| {
    let v = &args[0];
    let size = get_int(args, 1)? as usize;
    if size == 0 {
        return Err("collections.chunk: size must be > 0".to_string());
    }
    match v {
        Value::Array(a) => {
            let chunks: Vec<Value> = a.borrow().chunks(size).map(|c| Value::new_array(c.to_vec())).collect();
            Ok(Some(Value::new_array(chunks)))
        }
        _ => Err("collections.chunk: expected array".to_string()),
    }
});

coll_cap!(CollZipCap, "zip", 2, |args: &[Value]| {
    let a = match &args[0] {
        Value::Array(a) => a.clone(),
        _ => return Err("collections.zip: expected array".to_string()),
    };
    let b = match &args[1] {
        Value::Array(b) => b.clone(),
        _ => return Err("collections.zip: expected array".to_string()),
    };
    let pairs: Vec<Value> = a
        .borrow()
        .iter()
        .zip(b.borrow().iter())
        .map(|(x, y)| Value::new_array(vec![x.clone(), y.clone()]))
        .collect();
    Ok(Some(Value::new_array(pairs)))
});

coll_cap!(CollUniqueCap, "unique", 1, |args: &[Value]| {
    let v = &args[0];
    match v {
        Value::Array(a) => {
            let mut seen = Vec::new();
            let mut result = Vec::new();
            for item in a.borrow().iter() {
                if !seen.iter().any(|s| s == item) {
                    seen.push(item.clone());
                    result.push(item.clone());
                }
            }
            Ok(Some(Value::new_array(result)))
        }
        _ => Err("collections.unique: expected array".to_string()),
    }
});

// ─────────────────────────────────────────────────────────────────────────────
// JSON Capabilities
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "stdlib")]
use serde_json;

#[cfg(feature = "stdlib")]
macro_rules! json_cap {
    ($name:ident, $ic:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("json.", $ic).to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                $body(&args)
            }
        }
    };
}

// (The previous local `value_to_json` thin-wrapper that delegated to
// `crate::util::value_to_json` has been deleted. The canonical JSON
// conversion path is now `impl serde::Serialize for Value` on
// `crush_vm::vm::Value`; `serde_json::to_string(&args[0]).map_err(...)?`
// in the `json.stringify*` caps below invokes that trait impl
// directly — single source of truth, no helper needed.)

// (The previous local `json_to_value` thin-wrapper that duplicated
// the canonical JSON-from-`Value` parsing path has been deleted. The
// canonical inverse is now `impl serde::Deserialize for Value` in
// `crush-vm/src/vm.rs` (placed next to the existing `impl Serialize`
// and `impl Display` — three canonical traits on `Value`). `json.parse`
// below invokes `serde_json::from_str::<Value>(&s)` directly, so
// every JSON consumer (the cap, future parsers) goes through the
// single source of truth.
//
// Note: this also fixes a pre-existing bug in the deleted `json_to_value`
// where JSON objects were silently mapped to `Value::Null` ("// No map
// support yet"). The canonical Deserialize impl dispatches `visit_map`
// to `Value::new_map(...)` correctly, so JSON objects now parse into
// Crush maps without any per-call-site workaround.)

#[cfg(feature = "stdlib")]
json_cap!(JsonParseCap, "parse", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    // **Canonical path**: route through `impl serde::Deserialize for Value`
    // (defined on `crush_vm::vm::Value`) — no helper. `serde_json::from_str::<Value>`
    // invokes the trait impl directly via the visitor pattern.
    let parsed: Value =
        serde_json::from_str(&s).map_err(|e| format!("json.parse: {}", e))?;
    Ok(Some(parsed))
});

#[cfg(feature = "stdlib")]
json_cap!(JsonStringifyCap, "stringify", 1, |args: &[Value]| {
    // **Canonical path**: route through `impl serde::Serialize for Value`
    // (defined on `crush_vm::vm::Value`) — no helper. The previous local
    // `value_to_json` wrapper that delegated to `crate::util::value_to_json`
    // has been deleted; `serde_json::to_string(&args[0])` invokes the
    // trait impl directly so JSON produced here is identical to JSON
    // produced by `db.query` and `message_bus.publish`.
    let s = serde_json::to_string(&args[0])
        .map_err(|e| format!("json.stringify: {}", e))?;
    Ok(Some(Value::Str(s)))
});

#[cfg(feature = "stdlib")]
json_cap!(
    JsonStringifyPrettyCap,
    "stringify_pretty",
    1,
    |args: &[Value]| {
        // See `JsonStringifyCap` above for the canonical-path rationale.
        let s = serde_json::to_string_pretty(&args[0])
            .map_err(|e| format!("json.stringify_pretty: {}", e))?;
        Ok(Some(Value::Str(s)))
    }
);

// ─────────────────────────────────────────────────────────────────────────────
// Path Capabilities
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! path_cap {
    ($name:ident, $ic:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("path.", $ic).to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                $body(&args)
            }
        }
    };
}

path_cap!(PathJoinCap, "join", 2, |args: &[Value]| {
    let base = get_str(args, 0)?;
    let part = get_str(args, 1)?;
    let joined = std::path::Path::new(&base).join(&part);
    Ok(Some(Value::Str(joined.to_string_lossy().into_owned())))
});

path_cap!(PathDirnameCap, "dirname", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    let dir = std::path::Path::new(&p)
        .parent()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Some(Value::Str(dir)))
});

path_cap!(PathBasenameCap, "basename", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    let name = std::path::Path::new(&p)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Some(Value::Str(name)))
});

path_cap!(PathExtensionCap, "extension", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    let ext = std::path::Path::new(&p)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Some(Value::Str(ext)))
});

path_cap!(PathIsAbsoluteCap, "is_absolute", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    Ok(Some(Value::Int(
        if std::path::Path::new(&p).is_absolute() {
            1
        } else {
            0
        },
    )))
});

path_cap!(PathNormalizeCap, "normalize", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    let path = std::path::Path::new(&p);
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    let normalized: std::path::PathBuf = components.iter().collect();
    Ok(Some(Value::Str(normalized.to_string_lossy().into_owned())))
});

path_cap!(PathStemCap, "stem", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    let stem = std::path::Path::new(&p)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Some(Value::Str(stem)))
});

// ─────────────────────────────────────────────────────────────────────────────
// Regex Capabilities
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "stdlib")]
macro_rules! regex_cap {
    ($name:ident, $ic:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: concat!("regex.", $ic).to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                $body(&args)
            }
        }
    };
}

#[cfg(feature = "stdlib")]
regex_cap!(RegexTestCap, "test", 2, |args: &[Value]| {
    let pattern = get_str(args, 0)?;
    let text = get_str(args, 1)?;
    let re =
        regex::Regex::new(&pattern).map_err(|e| format!("regex.test: invalid pattern: {}", e))?;
    Ok(Some(Value::Int(if re.is_match(&text) { 1 } else { 0 })))
});

#[cfg(feature = "stdlib")]
regex_cap!(RegexMatchCap, "match", 2, |args: &[Value]| {
    let pattern = get_str(args, 0)?;
    let text = get_str(args, 1)?;
    let re =
        regex::Regex::new(&pattern).map_err(|e| format!("regex.match: invalid pattern: {}", e))?;
    let groups: Vec<Value> = re
        .captures(&text)
        .map(|caps| {
            caps.iter()
                .map(|m| {
                    m.map(|m| Value::Str(m.as_str().to_string()))
                        .unwrap_or(Value::Null)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(Value::new_array(groups)))
});

#[cfg(feature = "stdlib")]
regex_cap!(RegexFindAllCap, "find_all", 2, |args: &[Value]| {
    let pattern = get_str(args, 0)?;
    let text = get_str(args, 1)?;
    let re = regex::Regex::new(&pattern)
        .map_err(|e| format!("regex.find_all: invalid pattern: {}", e))?;
    let matches: Vec<Value> = re
        .find_iter(&text)
        .map(|m| Value::Str(m.as_str().to_string()))
        .collect();
    Ok(Some(Value::new_array(matches)))
});

#[cfg(feature = "stdlib")]
regex_cap!(RegexReplaceCap, "replace", 3, |args: &[Value]| {
    let pattern = get_str(args, 0)?;
    let text = get_str(args, 1)?;
    let replacement = get_str(args, 2)?;
    let re = regex::Regex::new(&pattern)
        .map_err(|e| format!("regex.replace: invalid pattern: {}", e))?;
    let result = re.replace_all(&text, replacement.as_str()).into_owned();
    Ok(Some(Value::Str(result)))
});

#[cfg(feature = "stdlib")]
regex_cap!(RegexSplitCap, "split", 2, |args: &[Value]| {
    let pattern = get_str(args, 0)?;
    let text = get_str(args, 1)?;
    let re =
        regex::Regex::new(&pattern).map_err(|e| format!("regex.split: invalid pattern: {}", e))?;
    let parts: Vec<Value> = re.split(&text).map(|s| Value::Str(s.to_string())).collect();
    Ok(Some(Value::new_array(parts)))
});

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use crush_vm::HostCaps;

    fn setup_caps() -> HostCaps {
        let mut caps = HostCaps::new();
        register(&mut caps);
        caps
    }

    #[test]
    fn test_str_split() {
        let caps = setup_caps();
        let cap = caps.get("str.split").unwrap();
        let result = cap
            .call(vec![
                Value::Str("a,b,c".to_string()),
                Value::Str(",".to_string()),
            ])
            .unwrap();
        match result {
            Some(Value::Array(arr)) => {
                assert_eq!(arr.borrow().len(), 3);
                assert_eq!(arr.borrow()[0], Value::Str("a".to_string()));
                assert_eq!(arr.borrow()[1], Value::Str("b".to_string()));
                assert_eq!(arr.borrow()[2], Value::Str("c".to_string()));
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_str_join() {
        let caps = setup_caps();
        let cap = caps.get("str.join").unwrap();
        let result = cap
            .call(vec![
                Value::new_array(vec![
                    Value::Str("a".to_string()),
                    Value::Str("b".to_string()),
                ]),
                Value::Str("-".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Str("a-b".to_string())));
    }

    #[test]
    fn test_str_trim() {
        let caps = setup_caps();
        let cap = caps.get("str.trim").unwrap();
        let result = cap.call(vec![Value::Str("  hello  ".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Str("hello".to_string())));
    }

    #[test]
    fn test_str_replace() {
        let caps = setup_caps();
        let cap = caps.get("str.replace").unwrap();
        let result = cap
            .call(vec![
                Value::Str("hello world".to_string()),
                Value::Str("world".to_string()),
                Value::Str("rust".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Str("hello rust".to_string())));
    }

    #[test]
    fn test_str_contains() {
        let caps = setup_caps();
        let cap = caps.get("str.contains").unwrap();
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Str("ell".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Int(1)));
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Str("xyz".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Int(0)));
    }

    #[test]
    fn test_str_starts_ends_with() {
        let caps = setup_caps();
        let cap = caps.get("str.starts_with").unwrap();
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Str("he".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        let cap = caps.get("str.ends_with").unwrap();
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Str("lo".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Int(1)));
    }

    #[test]
    fn test_str_to_upper_lower() {
        let caps = setup_caps();
        let cap = caps.get("str.to_upper").unwrap();
        let result = cap.call(vec![Value::Str("Hello".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Str("HELLO".to_string())));

        let cap = caps.get("str.to_lower").unwrap();
        let result = cap.call(vec![Value::Str("Hello".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Str("hello".to_string())));
    }

    #[test]
    fn test_str_pad() {
        let caps = setup_caps();
        let cap = caps.get("str.pad_left").unwrap();
        let result = cap
            .call(vec![
                Value::Str("5".to_string()),
                Value::Int(3),
                Value::Str("0".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Str("005".to_string())));

        let cap = caps.get("str.pad_right").unwrap();
        let result = cap
            .call(vec![
                Value::Str("5".to_string()),
                Value::Int(3),
                Value::Str("0".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Str("500".to_string())));
    }

    #[test]
    fn test_str_repeat() {
        let caps = setup_caps();
        let cap = caps.get("str.repeat").unwrap();
        let result = cap
            .call(vec![Value::Str("ab".to_string()), Value::Int(3)])
            .unwrap();
        assert_eq!(result, Some(Value::Str("ababab".to_string())));
    }

    #[test]
    fn test_str_substring() {
        let caps = setup_caps();
        let cap = caps.get("str.substring").unwrap();
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Int(1),
                Value::Int(4),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Str("ell".to_string())));
    }

    #[test]
    fn test_str_char_at() {
        let caps = setup_caps();
        let cap = caps.get("str.char_at").unwrap();
        let result = cap
            .call(vec![Value::Str("hello".to_string()), Value::Int(1)])
            .unwrap();
        assert_eq!(result, Some(Value::Str("e".to_string())));
    }

    #[test]
    fn test_str_index_of() {
        let caps = setup_caps();
        let cap = caps.get("str.index_of").unwrap();
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Str("l".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Int(2)));
        let result = cap
            .call(vec![
                Value::Str("hello".to_string()),
                Value::Str("z".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Int(-1)));
    }

    #[test]
    fn test_str_format() {
        let caps = setup_caps();
        let cap = caps.get("str.format").unwrap();
        let result = cap
            .call(vec![
                Value::Str("Hello {}!".to_string()),
                Value::Str("World".to_string()),
            ])
            .unwrap();
        assert_eq!(result, Some(Value::Str("Hello World!".to_string())));
    }

    #[test]
    fn test_math_sqrt() {
        let caps = setup_caps();
        let cap = caps.get("math.sqrt").unwrap();
        let result = cap.call(vec![Value::Int(16)]).unwrap();
        assert_eq!(result, Some(Value::Float(4.0)));
    }

    #[test]
    fn test_math_pow() {
        let caps = setup_caps();
        let cap = caps.get("math.pow").unwrap();
        let result = cap.call(vec![Value::Int(2), Value::Int(8)]).unwrap();
        assert_eq!(result, Some(Value::Float(256.0)));
    }

    #[test]
    fn test_math_abs() {
        let caps = setup_caps();
        let cap = caps.get("math.abs").unwrap();
        let result = cap.call(vec![Value::Int(-5)]).unwrap();
        assert_eq!(result, Some(Value::Float(5.0)));
    }

    #[test]
    fn test_math_floor_ceil_round() {
        let caps = setup_caps();
        let cap = caps.get("math.floor").unwrap();
        assert_eq!(
            cap.call(vec![Value::Float(3.7)]).unwrap(),
            Some(Value::Float(3.0))
        );
        let cap = caps.get("math.ceil").unwrap();
        assert_eq!(
            cap.call(vec![Value::Float(3.2)]).unwrap(),
            Some(Value::Float(4.0))
        );
        let cap = caps.get("math.round").unwrap();
        assert_eq!(
            cap.call(vec![Value::Float(3.5)]).unwrap(),
            Some(Value::Float(4.0))
        );
    }

    #[test]
    fn test_math_trig() {
        let caps = setup_caps();
        let cap = caps.get("math.sin").unwrap();
        let result = cap.call(vec![Value::Float(0.0)]).unwrap();
        assert!(
            (match result.unwrap() {
                Value::Float(f) => f,
                _ => 0.0,
            } - 0.0)
                .abs()
                < 0.001
        );

        let cap = caps.get("math.cos").unwrap();
        let result = cap.call(vec![Value::Float(0.0)]).unwrap();
        assert!(
            (match result.unwrap() {
                Value::Float(f) => f,
                _ => 0.0,
            } - 1.0)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn test_math_min_max() {
        let caps = setup_caps();
        let cap = caps.get("math.min").unwrap();
        assert_eq!(
            cap.call(vec![Value::Int(3), Value::Int(5)]).unwrap(),
            Some(Value::Float(3.0))
        );
        let cap = caps.get("math.max").unwrap();
        assert_eq!(
            cap.call(vec![Value::Int(3), Value::Int(5)]).unwrap(),
            Some(Value::Float(5.0))
        );
    }

    #[test]
    fn test_math_pi() {
        let caps = setup_caps();
        let cap = caps.get("math.pi").unwrap();
        let result = cap.call(vec![]).unwrap();
        assert!(
            (match result.unwrap() {
                Value::Float(f) => f,
                _ => 0.0,
            } - std::f64::consts::PI)
                .abs()
                < 0.001
        );
    }

    // Conversion tests
    #[test]
    fn test_conv_to_int() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_int").unwrap();
        assert_eq!(
            cap.call(vec![Value::Int(42)]).unwrap(),
            Some(Value::Int(42))
        );
        assert_eq!(
            cap.call(vec![Value::Float(3.14)]).unwrap(),
            Some(Value::Int(3))
        );
        assert_eq!(
            cap.call(vec![Value::Str("123".to_string())]).unwrap(),
            Some(Value::Int(123))
        );
        assert_eq!(cap.call(vec![Value::Null]).unwrap(), Some(Value::Int(0)));
    }

    #[test]
    fn test_conv_to_float() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_float").unwrap();
        assert_eq!(
            cap.call(vec![Value::Int(42)]).unwrap(),
            Some(Value::Float(42.0))
        );
        assert_eq!(
            cap.call(vec![Value::Float(3.14)]).unwrap(),
            Some(Value::Float(3.14))
        );
        assert_eq!(
            cap.call(vec![Value::Str("2.5".to_string())]).unwrap(),
            Some(Value::Float(2.5))
        );
    }

    #[test]
    fn test_conv_to_str() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_str").unwrap();
        assert_eq!(
            cap.call(vec![Value::Int(42)]).unwrap(),
            Some(Value::Str("42".to_string()))
        );
        assert_eq!(
            cap.call(vec![Value::Float(3.14)]).unwrap(),
            Some(Value::Str("3.14".to_string()))
        );
        // `Value::Null` now matches `io.print`'s formatter (literal `"null"`,
        // not `""`) — see `get_str` / `value_to_string` breadcrumbs in
        // `stdlib.rs`. Caps `str.join`, `str.format`, `conv.to_str`,
        // `path.*` all flow through this same path now.
        assert_eq!(
            cap.call(vec![Value::Null]).unwrap(),
            Some(Value::Str("null".to_string()))
        );
    }

    #[test]
    fn test_conv_to_bool() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_bool").unwrap();
        assert_eq!(cap.call(vec![Value::Int(1)]).unwrap(), Some(Value::Int(1)));
        assert_eq!(cap.call(vec![Value::Int(0)]).unwrap(), Some(Value::Int(0)));
        assert_eq!(
            cap.call(vec![Value::Str("hello".to_string())]).unwrap(),
            Some(Value::Int(1))
        );
        assert_eq!(
            cap.call(vec![Value::Str("".to_string())]).unwrap(),
            Some(Value::Int(0))
        );
        assert_eq!(cap.call(vec![Value::Null]).unwrap(), Some(Value::Int(0)));
        assert_eq!(
            cap.call(vec![Value::new_array(vec![])]).unwrap(),
            Some(Value::Int(0))
        );
        assert_eq!(
            cap.call(vec![Value::new_array(vec![Value::Int(1)])]).unwrap(),
            Some(Value::Int(1))
        );
    }

    #[test]
    fn test_conv_parse_int() {
        let caps = setup_caps();
        let cap = caps.get("conv.parse_int").unwrap();
        assert_eq!(
            cap.call(vec![Value::Str("123".to_string()), Value::Int(10)])
                .unwrap(),
            Some(Value::Int(123))
        );
        assert_eq!(
            cap.call(vec![Value::Str("FF".to_string()), Value::Int(16)])
                .unwrap(),
            Some(Value::Int(255))
        );
        assert_eq!(
            cap.call(vec![Value::Str("abc".to_string()), Value::Int(10)])
                .unwrap(),
            Some(Value::Null)
        );
    }

    #[test]
    fn test_conv_parse_float() {
        let caps = setup_caps();
        let cap = caps.get("conv.parse_float").unwrap();
        assert_eq!(
            cap.call(vec![Value::Str("3.14".to_string())]).unwrap(),
            Some(Value::Float(3.14))
        );
    }

    #[test]
    fn test_conv_type_of() {
        let caps = setup_caps();
        let cap = caps.get("conv.type_of").unwrap();
        assert_eq!(
            cap.call(vec![Value::Int(1)]).unwrap(),
            Some(Value::Str("int".to_string()))
        );
        assert_eq!(
            cap.call(vec![Value::Float(1.0)]).unwrap(),
            Some(Value::Str("float".to_string()))
        );
        assert_eq!(
            cap.call(vec![Value::Str("x".to_string())]).unwrap(),
            Some(Value::Str("string".to_string()))
        );
        assert_eq!(
            cap.call(vec![Value::Null]).unwrap(),
            Some(Value::Str("null".to_string()))
        );
        assert_eq!(
            cap.call(vec![Value::new_array(vec![])]).unwrap(),
            Some(Value::Str("array".to_string()))
        );
    }

    // Collections tests
    #[test]
    fn test_coll_len() {
        let caps = setup_caps();
        let cap = caps.get("collections.len").unwrap();
        assert_eq!(
            cap.call(vec![Value::new_array(vec![Value::Int(1), Value::Int(2)])])
                .unwrap(),
            Some(Value::Int(2))
        );
        assert_eq!(
            cap.call(vec![Value::Str("hello".to_string())]).unwrap(),
            Some(Value::Int(5))
        );
    }

    #[test]
    fn test_coll_reverse() {
        let caps = setup_caps();
        let cap = caps.get("collections.reverse").unwrap();
        let result = cap
            .call(vec![Value::new_array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
            ])])
            .unwrap();
        assert_eq!(
            result,
            Some(Value::new_array(vec![
                Value::Int(3),
                Value::Int(2),
                Value::Int(1)
            ]))
        );
    }

    #[test]
    fn test_coll_includes() {
        let caps = setup_caps();
        let cap = caps.get("collections.includes").unwrap();
        assert_eq!(
            cap.call(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::Int(2)
            ])
            .unwrap(),
            Some(Value::Int(1))
        );
        assert_eq!(
            cap.call(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::Int(3)
            ])
            .unwrap(),
            Some(Value::Int(0))
        );
    }

    #[test]
    fn test_coll_flatten() {
        let caps = setup_caps();
        let cap = caps.get("collections.flatten").unwrap();
        let result = cap
            .call(vec![Value::new_array(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::new_array(vec![Value::Int(3)]),
            ])])
            .unwrap();
        assert_eq!(
            result,
            Some(Value::new_array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]))
        );
    }

    #[test]
    fn test_coll_chunk() {
        let caps = setup_caps();
        let cap = caps.get("collections.chunk").unwrap();
        let result = cap
            .call(vec![
                Value::new_array(vec![
                    Value::Int(1),
                    Value::Int(2),
                    Value::Int(3),
                    Value::Int(4),
                ]),
                Value::Int(2),
            ])
            .unwrap();
        assert_eq!(
            result,
            Some(Value::new_array(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::new_array(vec![Value::Int(3), Value::Int(4)]),
            ]))
        );
    }

    #[test]
    fn test_coll_zip() {
        let caps = setup_caps();
        let cap = caps.get("collections.zip").unwrap();
        let result = cap
            .call(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(2)]),
                Value::new_array(vec![Value::Int(3), Value::Int(4)]),
            ])
            .unwrap();
        assert_eq!(
            result,
            Some(Value::new_array(vec![
                Value::new_array(vec![Value::Int(1), Value::Int(3)]),
                Value::new_array(vec![Value::Int(2), Value::Int(4)]),
            ]))
        );
    }

    #[test]
    fn test_coll_unique() {
        let caps = setup_caps();
        let cap = caps.get("collections.unique").unwrap();
        let result = cap
            .call(vec![Value::new_array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(1),
                Value::Int(3),
            ])])
            .unwrap();
        assert_eq!(
            result,
            Some(Value::new_array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]))
        );
    }

    // JSON tests
    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse() {
        let caps = setup_caps();
        let cap = caps.get("json.parse").unwrap();
        let result = cap
            .call(vec![Value::Str(r#"[1,2,3]"#.to_string())])
            .unwrap();
        // `array.len()` was previously called directly on the `Rc<RefCell<...>>`
        // which fails to compile (3 such errors were masked behind
        // `#[cfg(feature = "stdlib")]`). Fixed by routing through `.borrow()`.
        assert!(matches!(result, Some(Value::Array(arr)) if arr.borrow().len() == 3));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse_object() {
        // End-to-end lock for the canonical Deserialize path through
        // `json.parse` exercising a JSON object. The pre-extraction
        // `stdlib::json_to_value` had `serde_json::Value::Object(_) =>
        // Value::Null` (a documented "No map support yet" stub) which
        // silently dropped every JSON object into `Value::Null`,
        // destroying caller data. The canonical `impl
        // serde::Deserialize for Value` in `crush-vm/src/vm.rs::impl
        // Deserialize` dispatches `visit_map` to `Value::new_map(...)`
        // correctly; this test pins that fix CI-side.
        let caps = setup_caps();
        let cap = caps.get("json.parse").unwrap();
        let result = cap
            .call(vec![Value::Str(r#"{"k": 1, "k2": null}"#.to_string())])
            .unwrap();
        match result {
            Some(Value::Map(m)) => {
                let m = m.borrow();
                // 2 entries preserved, NOT collapsed to Null under
                // the deleted `json_to_value`'s `Object(_) => Null`
                // branch.
                assert_eq!(m.len(), 2, "expected 2 entries in parsed map, got {}", m.len());
                assert_eq!(
                    m.get("k").cloned().unwrap_or(Value::Null),
                    Value::Int(1),
                    "k should be Int(1)"
                );
                assert_eq!(
                    m.get("k2").cloned().unwrap_or(Value::Str("".to_string())),
                    Value::Null,
                    "k2 should be Null"
                );
            }
            Some(Value::Null) => panic!(
                "FAIL: json.parse(`{{\"k\": 1, \"k2\": null}}`) returned Null; \
                 this is the exact pre-existing bug the canonical Deserialize \
                 refactor was meant to fix — visit_map should route to Value::Map"
            ),
            other => panic!(
                "expected Value::Map from json.parse object input, got {:?}",
                other
            ),
        }
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse_object_round_trip() {
        // Round-trip lock: serialize a map via Display-style string
        // representation, parse it back via json.parse, and verify
        // round-trip identity. Locks the canonical Serialize ∘
        // Deserialize inverse for object inputs through the cap-gated
        // path — i.e., the same path `db.query` rows and
        // `message_bus.recv` payloads would take.
        let caps = setup_caps();
        let stringify = caps.get("json.stringify").unwrap();
        let parse = caps.get("json.parse").unwrap();

        let mut m = std::collections::HashMap::new();
        m.insert("name".to_string(), Value::Str("test".to_string()));
        m.insert("count".to_string(), Value::Int(42));
        m.insert("flag".to_string(), Value::Bool(true));
        let original = Value::new_map(m);

        let serialized = stringify.call(vec![original.clone()]).unwrap().unwrap();
        let serialized_str = match serialized {
            Value::Str(s) => s,
            other => panic!("json.stringify should produce Value::Str, got {:?}", other),
        };
        let parsed = parse.call(vec![Value::Str(serialized_str)]).unwrap();

        match parsed {
            Some(Value::Map(roundtripped)) => {
                let r = roundtripped.borrow();
                assert_eq!(r.len(), 3);
                assert_eq!(
                    r.get("name").cloned().unwrap_or(Value::Null),
                    Value::Str("test".to_string())
                );
                assert_eq!(
                    r.get("count").cloned().unwrap_or(Value::Null),
                    Value::Int(42)
                );
                assert_eq!(
                    r.get("flag").cloned().unwrap_or(Value::Null),
                    Value::Bool(true)
                );
            }
            other => panic!(
                "expected Value::Map from json.parse round-trip, got {:?}",
                other
            ),
        }
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse_nested_object() {
        // Locks `visit_map` recursion through the cap gateway: a JSON
        // object containing another JSON object hydrates into a nested
        // `Value::Map({Value::Map(...)})`. The canonical
        // `impl serde::Deserialize for Value` dispatches `visit_map`
        // once per `{...}` token, so the inner-object keys become a
        // child `HashMap<String, Value>` under the outer key. Without
        // `visit_map` recursion (e.g. a stubbed Deserialize impl,
        // pre-extraction `stdlib::json_to_value`'s
        // `Object(_) => Null` arm), the inner object would either
        // fail to parse or be silently downgraded — this fixture
        // ensures the recursive dispatch holds end-to-end through
        // the `json.parse` cap.
        let caps = setup_caps();
        let cap = caps.get("json.parse").unwrap();
        let result = cap
            .call(vec![Value::Str(r#"{"outer": {"inner": 1}}"#.to_string())])
            .unwrap();
        match result {
            Some(Value::Map(outer_rc)) => {
                let outer = outer_rc.borrow();
                assert_eq!(
                    outer.len(),
                    1,
                    "outer map should have 1 entry, got {}",
                    outer.len()
                );
                let inner_value = outer
                    .get("outer")
                    .cloned()
                    .expect("'outer' key should be present");
                match inner_value {
                    Value::Map(inner_rc) => {
                        let inner = inner_rc.borrow();
                        assert_eq!(
                            inner.len(),
                            1,
                            "inner map should have 1 entry, got {}",
                            inner.len()
                        );
                        assert_eq!(
                            inner.get("inner").cloned().unwrap_or(Value::Null),
                            Value::Int(1),
                            "outer.outer.inner should be Int(1)"
                        );
                    }
                    other => panic!(
                        "outer['outer'] should be Value::Map (nested), got {:?}",
                        other
                    ),
                }
            }
            Some(Value::Null) => panic!(
                "FAIL: json.parse(`{{\"outer\": {{\"inner\": 1}}}}`) returned Null; \
                 this is the exact pre-existing bug the canonical Deserialize \
                 refactor was meant to fix — visit_map recursion missing"
            ),
            other => panic!(
                "expected Value::Map from json.parse nested-object input, got {:?}",
                other
            ),
        }
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse_mixed_types_object() {
        // Locks `visit_seq` recursion plus family-mixing under the
        // cap gateway: a JSON object whose value is an array with
        // elements from three different `Value` families (`Int`,
        // `Str`, `Null`) hydrates as
        // `Value::Map({items: Value::Array([Int(1), Str("two"), Null])})`.
        // The flat-object test in `test_json_parse_object` covers
        // `Map` + `visit_map` at a single level; this fixture
        // extends coverage to `Map` → `visit_seq` → mixed-family
        // `visit_bool/i64/str/unit` dispatch in a single parse
        // pipeline. Each family is already covered individually by
        // `Value::Int` / `Value::Str` / `Value::Null` literals in
        // the matrix test, but the combined Map+Array+Int+Str+Null
        // shape is unique to this fixture.
        let caps = setup_caps();
        let cap = caps.get("json.parse").unwrap();
        let result = cap
            .call(vec![Value::Str(
                r#"{"items": [1, "two", null]}"#.to_string(),
            )])
            .unwrap();
        match result {
            Some(Value::Map(m_rc)) => {
                let m = m_rc.borrow();
                assert_eq!(
                    m.len(),
                    1,
                    "outer map should have 1 entry, got {}",
                    m.len()
                );
                let items_value = m
                    .get("items")
                    .cloned()
                    .expect("'items' key should be present");
                match items_value {
                    Value::Array(arr_rc) => {
                        let arr = arr_rc.borrow();
                        assert_eq!(
                            arr.len(),
                            3,
                            "items array should have 3 elements, got {}",
                            arr.len()
                        );
                        assert_eq!(
                            arr[0],
                            Value::Int(1),
                            "items[0] should be Int(1), got {:?}",
                            arr[0]
                        );
                        assert_eq!(
                            arr[1],
                            Value::Str("two".to_string()),
                            "items[1] should be Str(\"two\"), got {:?}",
                            arr[1]
                        );
                        assert_eq!(
                            arr[2],
                            Value::Null,
                            "items[2] should be Null, got {:?}",
                            arr[2]
                        );
                    }
                    other => panic!(
                        "items should be Value::Array (visit_seq recursion), got {:?}",
                        other
                    ),
                }
            }
            Some(Value::Null) => panic!(
                "FAIL: json.parse(`{{\"items\": [1, \"two\", null]}}`) returned Null; \
                 visit_map dispatch missing on object wrapper"
            ),
            other => panic!(
                "expected Value::Map from json.parse mixed-types input, got {:?}",
                other
            ),
        }
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse_tagged_forms() {
        // Locks the `visit_str` tagged-form precedence that mirrors
        // the canonical `impl serde::Serialize for Value` tagged-form
        // contract, exercised end-to-end through the `json.parse` cap
        // gateway. Each input is a JSON-quoted string literal
        // (`"<handle 42>"`), which serde_json strips the surrounding
        // JSON quotes from BEFORE calling `visit_str` — so visit_str
        // sees `<handle 42>` (no quotes), then matches the canonical
        // precedence.
        //
        // Precedence in `crush_vm::vm::Value::impl Deserialize::visit_str`:
        //   1. `<handle N>`  (most specific — exact prefix `<handle `)
        //   2. `<N bytes>`   (general `<...>` shape + ` bytes>` suffix)
        //   3. `error(msg)`  (literal prefix + literal suffix)
        //   4. `Value::Str`  fallback
        //
        // Without locked precedence, the canonical round-trip would
        // be broken: e.g. `Value::Handle(42).serialize()` produces
        // `"<handle 42>"`, and parsing `"<handle 42>"` should return
        // `Value::Handle(42)`, NOT `Value::Str("<handle 42>")` — the
        // exact symmetry-mirror the `all_traits_round_trip` matrix
        // asserts at the trait level, here locked end-to-end through
        // the cap (so a future cap-layer regression would surface
        // here, NOT silently break the trait invariant).
        let caps = setup_caps();
        let cap = caps.get("json.parse").unwrap();

        // 1. `<handle N>` → Value::Handle(N). Both ends of the
        //    round-trip (publish-side Serialize produces
        //    `"<handle 42>"`; receive-side Deserialize parses it back)
        //    flow through the `Handle` tag together.
        let result = cap
            .call(vec![Value::Str(r#""<handle 42>""#.to_string())])
            .unwrap();
        match result {
            Some(Value::Handle(id)) => assert_eq!(
                id, 42,
                "expected Handle(42) for `\"<handle 42>\"` input, got Handle({id})"
            ),
            Some(Value::Str(s)) => panic!(
                "FAIL: visit_str missed the `<handle N>` tag (precedence broken); \
                 fell through to Value::Str({s:?}) instead of Value::Handle(42)"
            ),
            other => panic!(
                "expected Value::Handle(42) for `\"<handle 42>\"` input, got {:?}",
                other
            ),
        }

        // 2. `<N bytes>` → Value::Bytes(vec![0; N]). Documented
        //    length-only caveat: canonical Serialize preserves only
        //    the byte COUNT (not the actual content), so the
        //    reconstructed Vec<u8> is zero-filled to that length.
        //    This is the exact inverse contract — round-trip
        //    identity holds only for all-zero `Value::Bytes`
        //    payloads, as documented in
        //    `crush-vm::vm::impl Deserialize`.
        let result = cap
            .call(vec![Value::Str(r#""<3 bytes>""#.to_string())])
            .unwrap();
        match result {
            Some(Value::Bytes(b)) => assert_eq!(
                b,
                vec![0, 0, 0],
                "expected Bytes(vec![0,0,0]) (zero-fill per the length-only caveat), got {:?}",
                b
            ),
            Some(Value::Str(s)) => panic!(
                "FAIL: visit_str missed the `<N bytes>` tag (precedence broken); \
                 fell through to Value::Str({s:?}) instead of Value::Bytes(vec![0,0,0])"
            ),
            other => panic!(
                "expected Value::Bytes(vec![0,0,0]) for `\"<3 bytes>\"` input, got {:?}",
                other
            ),
        }

        // 3. `error(msg)` → Value::Error(msg). The Display form
        //    `error(oops)` (no quotes) and the Serialize form
        //    `"error(oops)"` (JSON-quoted) are both canonical; the
        //    `visit_str` branch matches the inner literal `error(...)`
        //    shape and slices off the wrapping `error(` / `)` chars,
        //    yielding `Value::Error("oops")` from the inner `oops`
        //    payload.
        let result = cap
            .call(vec![Value::Str(r#""error(oops)""#.to_string())])
            .unwrap();
        match result {
            Some(Value::Error(msg)) => assert_eq!(
                msg, "oops",
                "expected Error(\"oops\") for `\"error(oops)\"` input, got Error({msg:?})"
            ),
            Some(Value::Str(s)) => panic!(
                "FAIL: visit_str missed the `error(msg)` tag (precedence broken); \
                 fell through to Value::Str({s:?}) instead of Value::Error(\"oops\")"
            ),
            other => panic!(
                "expected Value::Error(\"oops\") for `\"error(oops)\"` input, got {:?}",
                other
            ),
        }

        // 4. `error((foo)` → Value::Error("(foo"). Nested-paren
        //    boundary: the canonical Deserialize Error branch
        //    `v[6..v.len() - 1]` strips only ONE leading wrap
        //    (the prefix `error(`) and ONE trailing wrap (the
        //    suffix check `)`). The 11-char input `error((foo)`
        //    after the slice yields the 4-char payload `(foo` —
        //    the inner-most opening paren is preserved; the
        //    outer trailing close is consumed by the suffix
        //    check. This is NOT a balanced-paren walk; the
        //    asymmetry w.r.t. case 5 reflects the canonical
        //    `v[6..s.len() - 1]` slice arithmetic exactly.
        let result = cap
            .call(vec![Value::Str(r#""error((foo)""#.to_string())])
            .unwrap();
        match result {
            Some(Value::Error(msg)) => assert_eq!(
                msg, "(foo",
                "expected canonical 4-char slice '(foo' for `\"error((foo)\"` input \
                 (one leading `(` + one trailing `)` stripped, inner-most paren preserved), \
                 got Error({msg:?})"
            ),
            Some(Value::Str(s)) => panic!(
                "FAIL: visit_str missed the `error(msg)` tag for `\"error((foo)\"`; \
                 fell through to Value::Str({s:?}) instead of Value::Error(\"(foo\")"
            ),
            other => panic!(
                "expected Value::Error for `\"error((foo)\"` input, got {other:?}"
            ),
        }

        // 5. `error(foo))` → Value::Error("foo)"). Nested-paren
        //    boundary: same `v[6..v.len() - 1]` slice semantics
        //    as case 4. 11-char input `error(foo))` after the
        //    slice yields `foo)` (4 chars, ONE trailing `)` is
        //    consumed by the suffix check; the inner-most `)` is
        //    preserved). Symmetric in shape to case 4: the slice
        //    removes exactly one outer wrap, no balanced-paren
        //    walk. Locks the multi-close handler CI-side.
        let result = cap
            .call(vec![Value::Str(r#""error(foo))""#.to_string())])
            .unwrap();
        match result {
            Some(Value::Error(msg)) => assert_eq!(
                msg, "foo)",
                "expected canonical 4-char slice 'foo)' for `\"error(foo))\"` input \
                 (one trailing `)` stripped, inner-most closing paren preserved), \
                 got Error({msg:?})"
            ),
            Some(Value::Str(s)) => panic!(
                "FAIL: visit_str missed the `error(msg)` tag for `\"error(foo))\"`; \
                 fell through to Value::Str({s:?}) instead of Value::Error(\"foo)\")"
            ),
            other => panic!(
                "expected Value::Error for `\"error(foo))\"` input, got {other:?}"
            ),
        }

        // 6. Bytes round-trip is LOSSY by design. Canonical
        //    `impl Serialize for Value::Bytes(b)` emits ONLY the
        //    length-prefix inner-content `<{N} bytes>` (9 chars for
        //    N=3) — actual byte contents are NOT preserved through
        //    the JSON wire. The cap-layer string returned by
        //    `json.stringify` is the JSON-quoted form
        //    `"<3 bytes>"` (11 chars total: opening `"` + 9-char
        //    content + closing `"`) because `serde_json::to_string`
        //    wraps the trait-emitted blob with surrounding JSON
        //    quotes before returning. Re-parsing the recovered
        //    JSON-quoted tag through canonical
        //    `impl Deserialize for Value::visit_str` reconstructs
        //    a ZERO-FILLED `Vec<u8>` of the same length — NOT the
        //    original byte payload. This fixture exercises both
        //    caps to pin the documented length-only caveat
        //    CI-side; a future change to either side that
        //    restored byte fidelity would surface here as a
        //    panic on the assert_eq! calls.
        let stringify = caps.get("json.stringify").unwrap();
        let payload = vec![1u8, 2, 3];
        let serialized = stringify
            .call(vec![Value::Bytes(payload)])
            .unwrap()
            .expect("json.stringify produces Some");
        let serialized_str = match serialized {
            Value::Str(s) => s,
            other => panic!(
                "json.stringify should return Value::Str for byte payload, got {:?}",
                other
            ),
        };
        // Pin the Serialize-side output: byte CONTENTS dropped,
        // length preserved. The cap-layer output is the
        // JSON-quoted form `r#""<3 bytes>""#` (11 chars) — the
        // bare tag `<3 bytes>` (9 chars) emitted by the trait impl
        // is wrapped in JSON quotes by `serde_json::to_string`
        // BEFORE being returned to the cap caller.
        assert_eq!(
            serialized_str, r#""<3 bytes>""#,
            "canonical Serialize for Value::Bytes(vec![1,2,3]) at the cap layer should \
             emit the JSON-quoted length-tag \"<3 bytes>\" (byte contents intentionally \
             stripped), got {serialized_str:?}"
        );
        // Pin the Deserialize-side reconstruction: zero-filled.
        let parsed = cap
            .call(vec![Value::Str(serialized_str)])
            .unwrap();
        match parsed {
            Some(Value::Bytes(b)) => assert_eq!(
                b, vec![0u8, 0, 0],
                "LOSSY ROUND-TRIP: canonical Deserialize for \"<3 bytes>\" \
                 reconstructs a ZERO-FILLED Vec<u8> of length N, NOT the \
                 original byte payload vec![1,2,3]. Got {:?}, expected vec![0,0,0].",
                b
            ),
            Some(Value::Str(s)) => panic!(
                "FAIL: visit_str missed the `<N bytes>` tag in round-trip for \
                 Value::Bytes(vec![1,2,3]) -> \"<3 bytes>\" -> parse; \
                 fell through to Value::Str({s:?}) instead of zero-filled Value::Bytes"
            ),
            other => panic!(
                "expected Value::Bytes(vec![0,0,0]) after `\"<3 bytes>\"` round-trip, got {other:?}"
            ),
        }
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_bus_publish_recovers_handle_payload() {
        // Closes the loop on the `<handle N>` tagged form end-to-end
        // through the bus cap gateway. Fixtures 1 and 4 of
        // `test_json_parse_tagged_forms` cover the parse-side
        // reconstruction from a JSON-quoted tag string into a typed
        // `Value::Handle(N)`; this test covers the publish side
        // AND the recv-side re-serialization envelope path together.
        //
        // End-to-end loop:
        //   publish(Value::Handle(42))
        //     → canonical `impl Serialize for Value`
        //       → in-memory bus stores the JSON-quoted form `"<handle 42>"`
        //         → recv() pops
        //           → canonical `impl Deserialize for Value::visit_str`
        //             reconstructs `Value::Handle(42)`
        //             → canonical `impl Serialize for Value::new_map(envelope)`
        //               re-emits the envelope as a JSON string
        //                 → returned to the test caller as `Value::Str(...)`
        //                   → test re-parses via `json.parse`
        //                     → canonical `impl Deserialize for Value`
        //                       drills into `payload` slot
        //                         → MUST equal `Value::Handle(42)`.
        //
        // Both contract violations (the publish-side canonical
        // Serialize drifting away from `<handle N>`, OR the recv-side
        // canonical Deserialize missing the `<handle N>` precedence)
        // would surface here as a panic on the final `Value::Handle(_)`
        // match arm with the explicit `Value::Str` anti-regression
        // message.
        let mut caps = HostCaps::new();
        crate::stdlib::register(&mut caps);
        crate::bus::register(&mut caps);

        let publish = caps
            .get("message_bus.publish")
            .expect("message_bus.publish registered");
        let subscribe = caps
            .get("message_bus.subscribe")
            .expect("message_bus.subscribe registered");
        let recv = caps
            .get("message_bus.recv")
            .expect("message_bus.recv registered");
        let parse = caps.get("json.parse").expect("json.parse registered");

        // Subscribe FIRST so the in-memory bus maintains a queue for
        // "t_handle". The order subscribe → publish → recv is the same
        // sequence the production code path would take.
        subscribe
            .call(vec![Value::Str("t_handle".to_string())])
            .expect("message_bus.subscribe should succeed for t_handle");

        // Publish the `Value::Handle(42)` payload. The canonical
        // `impl Serialize for Value::Handle(42)` emits the bare tag
        // `<handle 42>` (11 chars). `serde_json::to_value` wraps the
        // trait-emitted blob in surrounding JSON quotes, storing
        // `serde_json::Value::String("<handle 42>")` (13 chars) in
        // the in-memory queue. publish returns `Ok(None)` per its
        // `HostCapSpec` (returns = false).
        let publish_result = publish
            .call(vec![
                Value::Str("t_handle".to_string()),
                Value::Handle(42),
            ])
            .expect("message_bus.publish should succeed");
        assert!(
            publish_result.is_none(),
            "message_bus.publish spec returns None (HostCapSpec.returns = false); \
             got {:?}",
            publish_result
        );

        // Recv the published message. The recv-side deserialize runs
        // the stored `serde_json::Value::String("<handle 42>")`
        // through canonical `impl Deserialize for Value::visit_str`,
        // which matches the `<handle N>` precedence and reconstructs
        // `Value::Handle(42)`. The cap wraps this typed payload with
        // the topic in a `Value::Map({topic, payload})`, then re-emits
        // via canonical `impl Serialize for Value` →
        // `serde_json::to_string` returns a JSON-quoted envelope
        // string. The cap returns `Value::Str(envelope_json_string)`.
        let recv_envelope = recv.call(vec![]).expect("message_bus.recv should succeed");
        let envelope_string = match recv_envelope {
            Some(Value::Str(s)) => s,
            other => panic!(
                "message_bus.recv should return Value::Str(JSON envelope), got {:?}",
                other
            ),
        };

        // Re-parse the envelope through `json.parse` to exercise the
        // canonical Deserialize path end-to-end. This is the
        // OUTERMOST loop-closure: the JSON envelope string is itself
        // JSON, so re-parsing it through the canonical Deserialize
        // path reconstructs the typed envelope map, with the payload
        // slot routed through `visit_str` once more. The two
        // `visit_str` passages (one inside recv, one inside
        // `json.parse`) MUST both hit the `<handle N>` precedence —
        // any drift in either layer would fail this match arm.
        let parsed_envelope = parse
            .call(vec![Value::Str(envelope_string)])
            .expect("json.parse of recv envelope should succeed");
        let envelope_map_rc = match parsed_envelope {
            Some(Value::Map(m_rc)) => m_rc,
            other => panic!(
                "json.parse of recv envelope should yield Value::Map envelope, got {:?}",
                other
            ),
        };
        let payload_value = {
            let envelope_map = envelope_map_rc.borrow();
            envelope_map
                .get("payload")
                .cloned()
                .expect("envelope map should contain 'payload' key")
        };

        // The regression-locking assertion: the canonical `<handle N>`
        // tagged form must round-trip through bus.publish → bus.recv
        // → json.parse as `Value::Handle(42)`, NOT fall through to
        // the generic-value path and land as `Value::Str("<handle 42>")`.
        // Drift in either the canonical Serialize (publish side) or
        // the canonical Deserialize (recv side or json.parse side)
        // would surface here with an explicit panic message.
        match payload_value {
            Value::Handle(id) => assert_eq!(
                id, 42,
                "expected Handle(42) end-to-end through message_bus.publish/recv \
                 + json.parse, got Handle({id})"
            ),
            Value::Str(s) => panic!(
                "FAIL: the `<handle N>` tagged form did NOT survive the end-to-end \
                 bus round-trip; payload reconstructed as Value::Str({s:?}) instead \
                 of Value::Handle(42). Likely cause: drift in canonical \
                 `impl Serialize for Value::Handle(N)` (publish-side) or canonical \
                 `impl Deserialize for Value::visit_str` (recv-side OR \
                 json.parse-side). Compare to fixtures 1 and 4 of \
                 `test_json_parse_tagged_forms` which lock the parse-side precedence."
            ),
            other => panic!(
                "expected Value::Handle(42) end-to-end through message_bus.publish/recv \
                 + json.parse, got {:?}",
                other
            ),
        }
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_stringify() {
        let caps = setup_caps();
        let cap = caps.get("json.stringify").unwrap();
        let result = cap
            .call(vec![Value::new_array(vec![Value::Int(1), Value::Int(2)])])
            .unwrap();
        assert_eq!(result, Some(Value::Str("[1,2]".to_string())));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_stringify_pretty() {
        let caps = setup_caps();
        let cap = caps.get("json.stringify_pretty").unwrap();
        let result = cap.call(vec![Value::new_array(vec![Value::Int(1)])]).unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert!(s.contains("1"));
    }

    // Path tests
    #[test]
    fn test_path_join() {
        let caps = setup_caps();
        let cap = caps.get("path.join").unwrap();
        let result = cap
            .call(vec![
                Value::Str("/home".to_string()),
                Value::Str("user".to_string()),
            ])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert!(s.ends_with("home/user"));
    }

    #[test]
    fn test_path_dirname() {
        let caps = setup_caps();
        let cap = caps.get("path.dirname").unwrap();
        let result = cap
            .call(vec![Value::Str("/home/user/file.txt".to_string())])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert_eq!(s, "/home/user");
    }

    #[test]
    fn test_path_basename() {
        let caps = setup_caps();
        let cap = caps.get("path.basename").unwrap();
        let result = cap
            .call(vec![Value::Str("/home/user/file.txt".to_string())])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert_eq!(s, "file.txt");
    }

    #[test]
    fn test_path_extension() {
        let caps = setup_caps();
        let cap = caps.get("path.extension").unwrap();
        let result = cap
            .call(vec![Value::Str("/home/user/file.txt".to_string())])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert_eq!(s, "txt");
    }

    #[test]
    fn test_path_is_absolute() {
        let caps = setup_caps();
        let cap = caps.get("path.is_absolute").unwrap();
        assert_eq!(
            cap.call(vec![Value::Str("/home".to_string())]).unwrap(),
            Some(Value::Int(1))
        );
        assert_eq!(
            cap.call(vec![Value::Str("home".to_string())]).unwrap(),
            Some(Value::Int(0))
        );
    }

    #[test]
    fn test_path_normalize() {
        let caps = setup_caps();
        let cap = caps.get("path.normalize").unwrap();
        let result = cap
            .call(vec![Value::Str("/home/user/../etc".to_string())])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert_eq!(s, "/home/etc");
    }

    #[test]
    fn test_path_stem() {
        let caps = setup_caps();
        let cap = caps.get("path.stem").unwrap();
        let result = cap
            .call(vec![Value::Str("/home/user/file.txt".to_string())])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert_eq!(s, "file");
    }

    // Regex tests
    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_test() {
        let caps = setup_caps();
        let cap = caps.get("regex.test").unwrap();
        assert_eq!(
            cap.call(vec![
                Value::Str(r"\d+".to_string()),
                Value::Str("123".to_string())
            ])
            .unwrap(),
            Some(Value::Int(1))
        );
        assert_eq!(
            cap.call(vec![
                Value::Str(r"\d+".to_string()),
                Value::Str("abc".to_string())
            ])
            .unwrap(),
            Some(Value::Int(0))
        );
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_match() {
        let caps = setup_caps();
        let cap = caps.get("regex.match").unwrap();
        let result = cap
            .call(vec![
                Value::Str(r"(\d+)-(\d+)".to_string()),
                Value::Str("123-456".to_string()),
            ])
            .unwrap();
        // See breadcrumb in `test_json_parse` above describing the
        // `.borrow()` fix for `Value::Array(arr)` matches! patterns.
        assert!(matches!(result, Some(Value::Array(arr)) if arr.borrow().len() >= 2));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_find_all() {
        let caps = setup_caps();
        let cap = caps.get("regex.find_all").unwrap();
        let result = cap
            .call(vec![
                Value::Str(r"\d+".to_string()),
                Value::Str("a1b23c4".to_string()),
            ])
            .unwrap();
        // See breadcrumb in `test_json_parse` above describing the
        // `.borrow()` fix for `Value::Array(arr)` matches! patterns.
        assert!(matches!(result, Some(Value::Array(arr)) if arr.borrow().len() == 3));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_replace() {
        let caps = setup_caps();
        let cap = caps.get("regex.replace").unwrap();
        let result = cap
            .call(vec![
                Value::Str(r"\d+".to_string()),
                Value::Str("a1b23c".to_string()),
                Value::Str("X".to_string()),
            ])
            .unwrap();
        let s = match result.unwrap() {
            Value::Str(s) => s,
            _ => String::new(),
        };
        assert_eq!(s, "aXbXc");
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_split() {
        let caps = setup_caps();
        let cap = caps.get("regex.split").unwrap();
        let result = cap
            .call(vec![
                Value::Str(r"\s+".to_string()),
                Value::Str("a  b   c".to_string()),
            ])
            .unwrap();
        assert_eq!(
            result,
            Some(Value::new_array(vec![
                Value::Str("a".to_string()),
                Value::Str("b".to_string()),
                Value::Str("c".to_string()),
            ]))
        );
    }
}
