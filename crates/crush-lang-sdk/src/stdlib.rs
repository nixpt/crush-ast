//! Standard library capabilities for CRUSH runtime.
//!
//! Pure computation capabilities ported from exosphere's stdlib.
//! These are stdcaps — always available, no capability gate required.

use crush_vm::{HostCap, HostCapSpec, HostCaps};
use crush_vm::vm::Value;

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
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Value::Null => String::new(),
            Value::Array(a) => a.iter().map(value_to_string).collect::<Vec<_>>().join(", "),
            Value::Map(m) => {
                let items: Vec<String> = m.iter().map(|(k, v)| format!("{}: {}", k, value_to_string(v))).collect();
                format!("{{{}}}", items.join(", "))
            }
            Value::Error(e) => format!("error({})", e),
            Value::Bytes(b) => format!("<{} bytes>", b.len()),
        })
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

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        Value::Null => String::new(),
        Value::Array(a) => a.iter().map(value_to_string).collect::<Vec<_>>().join(", "),
            Value::Map(m) => {
                let items: Vec<String> = m.iter().map(|(k, v)| format!("{}: {}", k, value_to_string(v))).collect();
                format!("{{{}}}", items.join(", "))
            }
            Value::Error(e) => format!("error({})", e),
            Value::Bytes(b) => format!("<{} bytes>", b.len()),
    }
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
    Ok(Some(Value::Array(parts)))
});

str_cap!(StrJoinCap, "join", 2, |args: &[Value]| {
    let arr = match &args[0] {
        Value::Array(a) => a,
        _ => return Err("str.join: first argument must be an array".to_string()),
    };
    let delim = get_str(args, 1)?;
    let strings: Result<Vec<String>, String> = arr.iter().map(|v| Ok(value_to_string(v))).collect();
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
    let padding: String = std::iter::repeat(pad_char).take(len - s.len()).collect();
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
    let padding: String = std::iter::repeat(pad_char).take(len - s.len()).collect();
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
        Value::Array(a) => Value::Int(if a.is_empty() { 0 } else { 1 }),
        Value::Map(m) => Value::Int(if m.is_empty() { 0 } else { 1 }),
        Value::Error(_) => Value::Int(1),
        Value::Bytes(b) => Value::Int(if b.is_empty() { 0 } else { 1 }),
    };
    Ok(Some(result))
});

conv_cap!(ConvParseIntCap, "parse_int", 2, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let radix = get_int(args, 1).unwrap_or(10) as u32;
    let result = i64::from_str_radix(s.trim(), radix).map(Value::Int).unwrap_or(Value::Null);
    Ok(Some(result))
});

conv_cap!(ConvParseFloatCap, "parse_float", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let result = s.trim().parse::<f64>().map(Value::Float).unwrap_or(Value::Null);
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
        Value::Array(a) => a.len(),
        Value::Str(s) => s.len(),
        _ => return Err("collections.len: expected array or string".to_string()),
    };
    Ok(Some(Value::Int(len as i64)))
});

coll_cap!(CollReverseCap, "reverse", 1, |args: &[Value]| {
    let v = &args[0];
    match v {
        Value::Array(a) => {
            let mut arr = a.clone();
            arr.reverse();
            Ok(Some(Value::Array(arr)))
        }
        _ => Err("collections.reverse: expected array".to_string()),
    }
});

coll_cap!(CollIncludesCap, "includes", 2, |args: &[Value]| {
    let v = &args[0];
    let needle = &args[1];
    let found = match v {
        Value::Array(a) => a.iter().any(|x| x == needle),
        _ => return Err("collections.includes: expected array".to_string()),
    };
    Ok(Some(Value::Int(if found { 1 } else { 0 })))
});

coll_cap!(CollFlattenCap, "flatten", 1, |args: &[Value]| {
    let v = &args[0];
    match v {
        Value::Array(a) => {
            let mut flat = Vec::new();
            for item in a {
                match item {
                    Value::Array(inner) => flat.extend(inner.clone()),
                    _ => flat.push(item.clone()),
                }
            }
            Ok(Some(Value::Array(flat)))
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
            let chunks: Vec<Value> = a
                .chunks(size)
                .map(|c| Value::Array(c.to_vec()))
                .collect();
            Ok(Some(Value::Array(chunks)))
        }
        _ => Err("collections.chunk: expected array".to_string()),
    }
});

coll_cap!(CollZipCap, "zip", 2, |args: &[Value]| {
    let a = match &args[0] { Value::Array(a) => a, _ => return Err("collections.zip: expected array".to_string()) };
    let b = match &args[1] { Value::Array(b) => b, _ => return Err("collections.zip: expected array".to_string()) };
    let pairs: Vec<Value> = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| Value::Array(vec![x.clone(), y.clone()]))
        .collect();
    Ok(Some(Value::Array(pairs)))
});

coll_cap!(CollUniqueCap, "unique", 1, |args: &[Value]| {
    let v = &args[0];
    match v {
        Value::Array(a) => {
            let mut seen = Vec::new();
            let mut result = Vec::new();
            for item in a {
                if !seen.iter().any(|s| s == item) {
                    seen.push(item.clone());
                    result.push(item.clone());
                }
            }
            Ok(Some(Value::Array(result)))
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

#[cfg(feature = "stdlib")]
fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::json!(*i),
        Value::Float(f) => serde_json::json!(*f),
        Value::Str(s) => serde_json::json!(s),
        Value::Array(a) => serde_json::Value::Array(a.iter().map(value_to_json).collect()),
    }
}

#[cfg(feature = "stdlib")]
fn json_to_value(val: serde_json::Value) -> Value {
    match val {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else { Value::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => Value::Str(s),
        serde_json::Value::Array(arr) => Value::Array(arr.into_iter().map(json_to_value).collect()),
        serde_json::Value::Object(_) => Value::Null, // No map support yet
    }
}

#[cfg(feature = "stdlib")]
json_cap!(JsonParseCap, "parse", 1, |args: &[Value]| {
    let s = get_str(args, 0)?;
    let parsed: serde_json::Value = serde_json::from_str(&s)
        .map_err(|e| format!("json.parse: {}", e))?;
    Ok(Some(json_to_value(parsed)))
});

#[cfg(feature = "stdlib")]
json_cap!(JsonStringifyCap, "stringify", 1, |args: &[Value]| {
    let json = value_to_json(&args[0]);
    let s = serde_json::to_string(&json)
        .map_err(|e| format!("json.stringify: {}", e))?;
    Ok(Some(Value::Str(s)))
});

#[cfg(feature = "stdlib")]
json_cap!(JsonStringifyPrettyCap, "stringify_pretty", 1, |args: &[Value]| {
    let json = value_to_json(&args[0]);
    let s = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("json.stringify_pretty: {}", e))?;
    Ok(Some(Value::Str(s)))
});

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
    Ok(Some(Value::Int(if std::path::Path::new(&p).is_absolute() { 1 } else { 0 })))
});

path_cap!(PathNormalizeCap, "normalize", 1, |args: &[Value]| {
    let p = get_str(args, 0)?;
    let path = std::path::Path::new(&p);
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => { components.pop(); }
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
    let re = regex::Regex::new(&pattern)
        .map_err(|e| format!("regex.test: invalid pattern: {}", e))?;
    Ok(Some(Value::Int(if re.is_match(&text) { 1 } else { 0 })))
});

#[cfg(feature = "stdlib")]
regex_cap!(RegexMatchCap, "match", 2, |args: &[Value]| {
    let pattern = get_str(args, 0)?;
    let text = get_str(args, 1)?;
    let re = regex::Regex::new(&pattern)
        .map_err(|e| format!("regex.match: invalid pattern: {}", e))?;
    let groups: Vec<Value> = re.captures(&text)
        .map(|caps| {
            caps.iter()
                .map(|m| m.map(|m| Value::Str(m.as_str().to_string())).unwrap_or(Value::Null))
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(Value::Array(groups)))
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
    Ok(Some(Value::Array(matches)))
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
    let re = regex::Regex::new(&pattern)
        .map_err(|e| format!("regex.split: invalid pattern: {}", e))?;
    let parts: Vec<Value> = re
        .split(&text)
        .map(|s| Value::Str(s.to_string()))
        .collect();
    Ok(Some(Value::Array(parts)))
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
        let result = cap.call(vec![Value::Str("a,b,c".to_string()), Value::Str(",".to_string())]).unwrap();
        match result {
            Some(Value::Array(arr)) => {
                assert_eq!(arr.len(), 3);
                assert_eq!(arr[0], Value::Str("a".to_string()));
                assert_eq!(arr[1], Value::Str("b".to_string()));
                assert_eq!(arr[2], Value::Str("c".to_string()));
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_str_join() {
        let caps = setup_caps();
        let cap = caps.get("str.join").unwrap();
        let result = cap.call(vec![
            Value::Array(vec![Value::Str("a".to_string()), Value::Str("b".to_string())]),
            Value::Str("-".to_string()),
        ]).unwrap();
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
        let result = cap.call(vec![
            Value::Str("hello world".to_string()),
            Value::Str("world".to_string()),
            Value::Str("rust".to_string()),
        ]).unwrap();
        assert_eq!(result, Some(Value::Str("hello rust".to_string())));
    }

    #[test]
    fn test_str_contains() {
        let caps = setup_caps();
        let cap = caps.get("str.contains").unwrap();
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Str("ell".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Int(1)));
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Str("xyz".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Int(0)));
    }

    #[test]
    fn test_str_starts_ends_with() {
        let caps = setup_caps();
        let cap = caps.get("str.starts_with").unwrap();
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Str("he".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Int(1)));

        let cap = caps.get("str.ends_with").unwrap();
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Str("lo".to_string())]).unwrap();
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
        let result = cap.call(vec![Value::Str("5".to_string()), Value::Int(3), Value::Str("0".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Str("005".to_string())));

        let cap = caps.get("str.pad_right").unwrap();
        let result = cap.call(vec![Value::Str("5".to_string()), Value::Int(3), Value::Str("0".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Str("500".to_string())));
    }

    #[test]
    fn test_str_repeat() {
        let caps = setup_caps();
        let cap = caps.get("str.repeat").unwrap();
        let result = cap.call(vec![Value::Str("ab".to_string()), Value::Int(3)]).unwrap();
        assert_eq!(result, Some(Value::Str("ababab".to_string())));
    }

    #[test]
    fn test_str_substring() {
        let caps = setup_caps();
        let cap = caps.get("str.substring").unwrap();
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Int(1), Value::Int(4)]).unwrap();
        assert_eq!(result, Some(Value::Str("ell".to_string())));
    }

    #[test]
    fn test_str_char_at() {
        let caps = setup_caps();
        let cap = caps.get("str.char_at").unwrap();
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Int(1)]).unwrap();
        assert_eq!(result, Some(Value::Str("e".to_string())));
    }

    #[test]
    fn test_str_index_of() {
        let caps = setup_caps();
        let cap = caps.get("str.index_of").unwrap();
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Str("l".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Int(2)));
        let result = cap.call(vec![Value::Str("hello".to_string()), Value::Str("z".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Int(-1)));
    }

    #[test]
    fn test_str_format() {
        let caps = setup_caps();
        let cap = caps.get("str.format").unwrap();
        let result = cap.call(vec![Value::Str("Hello {}!".to_string()), Value::Str("World".to_string())]).unwrap();
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
        assert_eq!(cap.call(vec![Value::Float(3.7)]).unwrap(), Some(Value::Float(3.0)));
        let cap = caps.get("math.ceil").unwrap();
        assert_eq!(cap.call(vec![Value::Float(3.2)]).unwrap(), Some(Value::Float(4.0)));
        let cap = caps.get("math.round").unwrap();
        assert_eq!(cap.call(vec![Value::Float(3.5)]).unwrap(), Some(Value::Float(4.0)));
    }

    #[test]
    fn test_math_trig() {
        let caps = setup_caps();
        let cap = caps.get("math.sin").unwrap();
        let result = cap.call(vec![Value::Float(0.0)]).unwrap();
        assert!((match result.unwrap() { Value::Float(f) => f, _ => 0.0 } - 0.0).abs() < 0.001);

        let cap = caps.get("math.cos").unwrap();
        let result = cap.call(vec![Value::Float(0.0)]).unwrap();
        assert!((match result.unwrap() { Value::Float(f) => f, _ => 0.0 } - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_math_min_max() {
        let caps = setup_caps();
        let cap = caps.get("math.min").unwrap();
        assert_eq!(cap.call(vec![Value::Int(3), Value::Int(5)]).unwrap(), Some(Value::Float(3.0)));
        let cap = caps.get("math.max").unwrap();
        assert_eq!(cap.call(vec![Value::Int(3), Value::Int(5)]).unwrap(), Some(Value::Float(5.0)));
    }

    #[test]
    fn test_math_pi() {
        let caps = setup_caps();
        let cap = caps.get("math.pi").unwrap();
        let result = cap.call(vec![]).unwrap();
        assert!((match result.unwrap() { Value::Float(f) => f, _ => 0.0 } - std::f64::consts::PI).abs() < 0.001);
    }

    // Conversion tests
    #[test]
    fn test_conv_to_int() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_int").unwrap();
        assert_eq!(cap.call(vec![Value::Int(42)]).unwrap(), Some(Value::Int(42)));
        assert_eq!(cap.call(vec![Value::Float(3.14)]).unwrap(), Some(Value::Int(3)));
        assert_eq!(cap.call(vec![Value::Str("123".to_string())]).unwrap(), Some(Value::Int(123)));
        assert_eq!(cap.call(vec![Value::Null]).unwrap(), Some(Value::Int(0)));
    }

    #[test]
    fn test_conv_to_float() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_float").unwrap();
        assert_eq!(cap.call(vec![Value::Int(42)]).unwrap(), Some(Value::Float(42.0)));
        assert_eq!(cap.call(vec![Value::Float(3.14)]).unwrap(), Some(Value::Float(3.14)));
        assert_eq!(cap.call(vec![Value::Str("2.5".to_string())]).unwrap(), Some(Value::Float(2.5)));
    }

    #[test]
    fn test_conv_to_str() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_str").unwrap();
        assert_eq!(cap.call(vec![Value::Int(42)]).unwrap(), Some(Value::Str("42".to_string())));
        assert_eq!(cap.call(vec![Value::Float(3.14)]).unwrap(), Some(Value::Str("3.14".to_string())));
        assert_eq!(cap.call(vec![Value::Null]).unwrap(), Some(Value::Str("".to_string())));
    }

    #[test]
    fn test_conv_to_bool() {
        let caps = setup_caps();
        let cap = caps.get("conv.to_bool").unwrap();
        assert_eq!(cap.call(vec![Value::Int(1)]).unwrap(), Some(Value::Int(1)));
        assert_eq!(cap.call(vec![Value::Int(0)]).unwrap(), Some(Value::Int(0)));
        assert_eq!(cap.call(vec![Value::Str("hello".to_string())]).unwrap(), Some(Value::Int(1)));
        assert_eq!(cap.call(vec![Value::Str("".to_string())]).unwrap(), Some(Value::Int(0)));
        assert_eq!(cap.call(vec![Value::Null]).unwrap(), Some(Value::Int(0)));
        assert_eq!(cap.call(vec![Value::Array(vec![])]).unwrap(), Some(Value::Int(0)));
        assert_eq!(cap.call(vec![Value::Array(vec![Value::Int(1)])]).unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn test_conv_parse_int() {
        let caps = setup_caps();
        let cap = caps.get("conv.parse_int").unwrap();
        assert_eq!(cap.call(vec![Value::Str("123".to_string()), Value::Int(10)]).unwrap(), Some(Value::Int(123)));
        assert_eq!(cap.call(vec![Value::Str("FF".to_string()), Value::Int(16)]).unwrap(), Some(Value::Int(255)));
        assert_eq!(cap.call(vec![Value::Str("abc".to_string()), Value::Int(10)]).unwrap(), Some(Value::Null));
    }

    #[test]
    fn test_conv_parse_float() {
        let caps = setup_caps();
        let cap = caps.get("conv.parse_float").unwrap();
        assert_eq!(cap.call(vec![Value::Str("3.14".to_string())]).unwrap(), Some(Value::Float(3.14)));
    }

    #[test]
    fn test_conv_type_of() {
        let caps = setup_caps();
        let cap = caps.get("conv.type_of").unwrap();
        assert_eq!(cap.call(vec![Value::Int(1)]).unwrap(), Some(Value::Str("int".to_string())));
        assert_eq!(cap.call(vec![Value::Float(1.0)]).unwrap(), Some(Value::Str("float".to_string())));
        assert_eq!(cap.call(vec![Value::Str("x".to_string())]).unwrap(), Some(Value::Str("string".to_string())));
        assert_eq!(cap.call(vec![Value::Null]).unwrap(), Some(Value::Str("null".to_string())));
        assert_eq!(cap.call(vec![Value::Array(vec![])]).unwrap(), Some(Value::Str("array".to_string())));
    }

    // Collections tests
    #[test]
    fn test_coll_len() {
        let caps = setup_caps();
        let cap = caps.get("collections.len").unwrap();
        assert_eq!(cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2)])]).unwrap(), Some(Value::Int(2)));
        assert_eq!(cap.call(vec![Value::Str("hello".to_string())]).unwrap(), Some(Value::Int(5)));
    }

    #[test]
    fn test_coll_reverse() {
        let caps = setup_caps();
        let cap = caps.get("collections.reverse").unwrap();
        let result = cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])]).unwrap();
        assert_eq!(result, Some(Value::Array(vec![Value::Int(3), Value::Int(2), Value::Int(1)])));
    }

    #[test]
    fn test_coll_includes() {
        let caps = setup_caps();
        let cap = caps.get("collections.includes").unwrap();
        assert_eq!(cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2)]), Value::Int(2)]).unwrap(), Some(Value::Int(1)));
        assert_eq!(cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2)]), Value::Int(3)]).unwrap(), Some(Value::Int(0)));
    }

    #[test]
    fn test_coll_flatten() {
        let caps = setup_caps();
        let cap = caps.get("collections.flatten").unwrap();
        let result = cap.call(vec![Value::Array(vec![
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(3)]),
        ])]).unwrap();
        assert_eq!(result, Some(Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])));
    }

    #[test]
    fn test_coll_chunk() {
        let caps = setup_caps();
        let cap = caps.get("collections.chunk").unwrap();
        let result = cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4)]), Value::Int(2)]).unwrap();
        assert_eq!(result, Some(Value::Array(vec![
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(3), Value::Int(4)]),
        ])));
    }

    #[test]
    fn test_coll_zip() {
        let caps = setup_caps();
        let cap = caps.get("collections.zip").unwrap();
        let result = cap.call(vec![
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(3), Value::Int(4)]),
        ]).unwrap();
        assert_eq!(result, Some(Value::Array(vec![
            Value::Array(vec![Value::Int(1), Value::Int(3)]),
            Value::Array(vec![Value::Int(2), Value::Int(4)]),
        ])));
    }

    #[test]
    fn test_coll_unique() {
        let caps = setup_caps();
        let cap = caps.get("collections.unique").unwrap();
        let result = cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(1), Value::Int(3)])]).unwrap();
        assert_eq!(result, Some(Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])));
    }

    // JSON tests
    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_parse() {
        let caps = setup_caps();
        let cap = caps.get("json.parse").unwrap();
        let result = cap.call(vec![Value::Str(r#"[1,2,3]"#.to_string())]).unwrap();
        assert!(matches!(result, Some(Value::Array(arr)) if arr.len() == 3));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_stringify() {
        let caps = setup_caps();
        let cap = caps.get("json.stringify").unwrap();
        let result = cap.call(vec![Value::Array(vec![Value::Int(1), Value::Int(2)])]).unwrap();
        assert_eq!(result, Some(Value::Str("[1,2]".to_string())));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_json_stringify_pretty() {
        let caps = setup_caps();
        let cap = caps.get("json.stringify_pretty").unwrap();
        let result = cap.call(vec![Value::Array(vec![Value::Int(1)])]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert!(s.contains("1"));
    }

    // Path tests
    #[test]
    fn test_path_join() {
        let caps = setup_caps();
        let cap = caps.get("path.join").unwrap();
        let result = cap.call(vec![Value::Str("/home".to_string()), Value::Str("user".to_string())]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert!(s.ends_with("home/user"));
    }

    #[test]
    fn test_path_dirname() {
        let caps = setup_caps();
        let cap = caps.get("path.dirname").unwrap();
        let result = cap.call(vec![Value::Str("/home/user/file.txt".to_string())]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert_eq!(s, "/home/user");
    }

    #[test]
    fn test_path_basename() {
        let caps = setup_caps();
        let cap = caps.get("path.basename").unwrap();
        let result = cap.call(vec![Value::Str("/home/user/file.txt".to_string())]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert_eq!(s, "file.txt");
    }

    #[test]
    fn test_path_extension() {
        let caps = setup_caps();
        let cap = caps.get("path.extension").unwrap();
        let result = cap.call(vec![Value::Str("/home/user/file.txt".to_string())]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert_eq!(s, "txt");
    }

    #[test]
    fn test_path_is_absolute() {
        let caps = setup_caps();
        let cap = caps.get("path.is_absolute").unwrap();
        assert_eq!(cap.call(vec![Value::Str("/home".to_string())]).unwrap(), Some(Value::Int(1)));
        assert_eq!(cap.call(vec![Value::Str("home".to_string())]).unwrap(), Some(Value::Int(0)));
    }

    #[test]
    fn test_path_normalize() {
        let caps = setup_caps();
        let cap = caps.get("path.normalize").unwrap();
        let result = cap.call(vec![Value::Str("/home/user/../etc".to_string())]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert_eq!(s, "/home/etc");
    }

    #[test]
    fn test_path_stem() {
        let caps = setup_caps();
        let cap = caps.get("path.stem").unwrap();
        let result = cap.call(vec![Value::Str("/home/user/file.txt".to_string())]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert_eq!(s, "file");
    }

    // Regex tests
    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_test() {
        let caps = setup_caps();
        let cap = caps.get("regex.test").unwrap();
        assert_eq!(cap.call(vec![Value::Str(r"\d+".to_string()), Value::Str("123".to_string())]).unwrap(), Some(Value::Int(1)));
        assert_eq!(cap.call(vec![Value::Str(r"\d+".to_string()), Value::Str("abc".to_string())]).unwrap(), Some(Value::Int(0)));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_match() {
        let caps = setup_caps();
        let cap = caps.get("regex.match").unwrap();
        let result = cap.call(vec![Value::Str(r"(\d+)-(\d+)".to_string()), Value::Str("123-456".to_string())]).unwrap();
        assert!(matches!(result, Some(Value::Array(arr)) if arr.len() >= 2));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_find_all() {
        let caps = setup_caps();
        let cap = caps.get("regex.find_all").unwrap();
        let result = cap.call(vec![Value::Str(r"\d+".to_string()), Value::Str("a1b23c4".to_string())]).unwrap();
        assert!(matches!(result, Some(Value::Array(arr)) if arr.len() == 3));
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_replace() {
        let caps = setup_caps();
        let cap = caps.get("regex.replace").unwrap();
        let result = cap.call(vec![
            Value::Str(r"\d+".to_string()),
            Value::Str("a1b23c".to_string()),
            Value::Str("X".to_string()),
        ]).unwrap();
        let s = match result.unwrap() { Value::Str(s) => s, _ => String::new() };
        assert_eq!(s, "aXbXc");
    }

    #[cfg(feature = "stdlib")]
    #[test]
    fn test_regex_split() {
        let caps = setup_caps();
        let cap = caps.get("regex.split").unwrap();
        let result = cap.call(vec![Value::Str(r"\s+".to_string()), Value::Str("a  b   c".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Array(vec![
            Value::Str("a".to_string()),
            Value::Str("b".to_string()),
            Value::Str("c".to_string()),
        ])));
    }
}