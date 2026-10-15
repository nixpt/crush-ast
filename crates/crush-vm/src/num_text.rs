//! Language-styled value → text (`conv.text(value, style)`).
//!
//! Crush prints a whole float as `6.0`. JavaScript (`console.log`) prints `6`
//! and switches to exponent form outside 1e-6 <= |x| < 1e21; Rust's `{}`
//! prints `6` and never uses an exponent. Walkers wrap printed values in
//! `conv.text` with their language's style. One implementation here; the AOT
//! runtimes mirror [`float_text`].

use crate::vm::{Value, VmError};

/// A float the way `style` (`"js"` or `"rust"`) prints it.
pub fn float_text(f: f64, style: &str) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return match (style, f > 0.0) {
            ("js", true) => "Infinity",
            ("js", false) => "-Infinity",
            (_, true) => "inf",
            (_, false) => "-inf",
        }
        .to_string();
    }
    let a = f.abs();
    if style == "js" && a != 0.0 && !(1e-6..1e21).contains(&a) {
        // Rust `{:e}` is the shortest digits as `1.5e-7`; JS writes `1.5e-7` /
        // `1e+21` (explicit `+`, no zero padding).
        let e = format!("{f:e}");
        let (mant, exp) = e.split_once('e').expect("exponent form");
        return if exp.starts_with('-') { format!("{mant}e{exp}") } else { format!("{mant}e+{exp}") };
    }
    // Rust's `{}`: shortest round-trip digits, decimal, `6` not `6.0`, `-0`.
    format!("{f}")
}

/// `conv.text(value, style)`: numbers in the language's style, strings as-is,
/// everything else as crush prints it.
pub fn text_as(v: &Value, style: &Value) -> Result<Value, VmError> {
    let style = match style {
        Value::Str(s) if s == "js" || s == "rust" => s.as_str(),
        other => return Err(VmError::TypeError { expected: "style \"js\" or \"rust\"", got: other.type_name() }),
    };
    Ok(Value::Str(match v {
        Value::Str(s) => s.clone(),
        Value::Float(f) => float_text(*f, style),
        other => other.as_text(),
    }))
}

#[cfg(test)]
mod tests {
    use super::float_text;

    #[test]
    fn js_matches_node() {
        // node -e 'console.log(7/2, 4/2, 1.5*4, 0.1+0.2, 1e21, 1e20, 1e-6, 1e-7, -0, 0/0, 1/0, -1.5e-7)'
        let got: Vec<String> = [3.5, 2.0, 6.0, 0.1 + 0.2, 1e21, 1e20, 1e-6, 1e-7, -0.0, f64::NAN, f64::INFINITY, -1.5e-7]
            .iter().map(|f| float_text(*f, "js")).collect();
        assert_eq!(got.join(" "), "3.5 2 6 0.30000000000000004 1e+21 100000000000000000000 0.000001 1e-7 -0 NaN Infinity -1.5e-7");
    }

    #[test]
    fn rust_matches_rustc() {
        let got: Vec<String> = [3.5, 2.0, 6.0, 0.1 + 0.2, 1e21, 1e-7, -0.0, f64::NAN, f64::INFINITY, 1e16]
            .iter().map(|f| float_text(*f, "rust")).collect();
        assert_eq!(got.join(" "), "3.5 2 6 0.30000000000000004 1000000000000000000000 0.0000001 -0 NaN inf 10000000000000000");
    }
}
