//! Helpers for the portable capabilities built into `crush-vm`.
//!
//! These functions mirror the behaviour of the VM's hard-coded capability
//! dispatch so that host code can pre-validate or post-process capability
//! calls without running a full program.

use crush_vm::vm::Value;

/// Errors from capability helper operations.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("capability {cap} expects {expected} argument(s), got {got}")]
    Arity {
        cap: &'static str,
        expected: usize,
        got: usize,
    },

    #[error("capability {cap} received argument of wrong type: {detail}")]
    Type { cap: &'static str, detail: String },
}

/// Render a slice of CVM1 values as text, the same way `io.print` and
/// `str.concat` do inside the VM.
pub fn print(args: &[Value]) -> String {
    args.iter().map(|v| value_as_text(v)).collect::<Vec<_>>().concat()
}

/// Alias for [`print`], matching the `str.concat` capability semantics.
pub fn concat(args: &[Value]) -> String {
    print(args)
}

/// Compute the byte length of a value's text representation, matching
/// `str.len`.
pub fn len(value: &Value) -> Result<i64, CapabilityError> {
    Ok(value_as_text(value).len() as i64)
}

/// Render a single CVM1 value as text, matching the VM's `Value::as_text`.
pub fn value_as_text(value: &crush_vm::vm::Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Array(a) => {
            let inner: Vec<_> = a.iter().map(value_as_text).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Map(m) => {
            let items: Vec<String> = m.iter().map(|(k, v)| format!("{}: {}", k, value_as_text(v))).collect();
            format!("{{{}}}", items.join(", "))
        }
        Value::Error(e) => format!("error({})", e),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
    }
}

/// Convert a text representation back into a CVM1 value. Integers and floats
/// are parsed; everything else becomes a string.
pub fn text_as_value(text: &str) -> Value {
    if text == "null" {
        return Value::Null;
    }
    if let Ok(i) = text.parse::<i64>() {
        return Value::Int(i);
    }
    if let Ok(f) = text.parse::<f64>() {
        return Value::Float(f);
    }
    Value::Str(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_concatenates_values() {
        let args = vec![
            Value::Str("hello ".to_string()),
            Value::Int(42),
            Value::Null,
        ];
        assert_eq!(print(&args), "hello 42null");
        assert_eq!(concat(&args), "hello 42null");
    }

    #[test]
    fn len_counts_bytes() {
        assert_eq!(len(&Value::Str("abc".to_string())).unwrap(), 3);
        assert_eq!(len(&Value::Int(12345)).unwrap(), 5);
    }

    #[test]
    fn float_without_fraction_has_decimal() {
        assert_eq!(value_as_text(&Value::Float(3.0)), "3.0");
    }

    #[test]
    fn text_as_value_roundtrip() {
        assert_eq!(text_as_value("null"), Value::Null);
        assert_eq!(text_as_value("42"), Value::Int(42));
        assert_eq!(text_as_value("3.14"), Value::Float(3.14));
        assert_eq!(text_as_value("foo"), Value::Str("foo".to_string()));
    }
}
