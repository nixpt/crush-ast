//! Canonical arithmetic semantics shared by the Crush VM backends.
//!
//! This module is the single source of truth for ADD, SUB, MUL, DIV, MOD,
//! NEG and numeric comparisons (LT/GT/LE/GE) on `Value`.  All backends that
//! operate on `Value` (scheduler, portable_vm, and any future backend)
//! should route their arithmetic opcodes through these helpers so they
//! cannot drift again.
//!
//! Canonical rules:
//!   * ADD concatenates strings when either operand is a string.
//!   * Otherwise both operands must be numeric (Int or Float).
//!   * If any operand is Float, the operation is performed as Float.
//!   * Int ADD/SUB/MUL use checked arithmetic and raise ArithmeticOverflow.
//!   * Int/Float DIV/MOD check for a zero divisor and raise DivByZero.
//!   * Int DIV truncates toward zero; int MOD is the matching remainder.
//!   * NEG is numeric only and checks for i64::MIN overflow.
//!   * Comparisons promote mixed int/float to float.

use crate::vm::{Value, VmError};

#[inline]
fn is_float(v: &Value) -> bool {
    matches!(v, Value::Float(_))
}

#[inline]
fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

#[inline]
fn to_i64(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i64,
        _ => 0,
    }
}

fn require_numeric(a: &Value, b: &Value) -> Result<(), VmError> {
    if !a.is_numeric() {
        return Err(VmError::TypeError {
            expected: "numeric",
            got: a.type_name(),
        });
    }
    if !b.is_numeric() {
        return Err(VmError::TypeError {
            expected: "numeric",
            got: b.type_name(),
        });
    }
    Ok(())
}

/// ADD with string concatenation when either side is a string.
pub fn add_values(a: &Value, b: &Value) -> Result<Value, VmError> {
    if matches!(a, Value::Str(_)) || matches!(b, Value::Str(_)) {
        return Ok(Value::Str(format!("{}{}", a.as_text(), b.as_text())));
    }
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        Ok(Value::Float(to_f64(a) + to_f64(b)))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        let res = ai.checked_add(bi).ok_or(VmError::ArithmeticOverflow)?;
        Ok(Value::Int(res))
    }
}

pub fn sub_values(a: &Value, b: &Value) -> Result<Value, VmError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        Ok(Value::Float(to_f64(a) - to_f64(b)))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        let res = ai.checked_sub(bi).ok_or(VmError::ArithmeticOverflow)?;
        Ok(Value::Int(res))
    }
}

pub fn mul_values(a: &Value, b: &Value) -> Result<Value, VmError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        Ok(Value::Float(to_f64(a) * to_f64(b)))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        let res = ai.checked_mul(bi).ok_or(VmError::ArithmeticOverflow)?;
        Ok(Value::Int(res))
    }
}

pub fn div_values(a: &Value, b: &Value) -> Result<Value, VmError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        let bf = to_f64(b);
        if bf == 0.0 {
            return Err(VmError::DivByZero);
        }
        Ok(Value::Float(to_f64(a) / bf))
    } else {
        let bi = to_i64(b);
        if bi == 0 {
            return Err(VmError::DivByZero);
        }
        Ok(Value::Int(to_i64(a) / bi))
    }
}

pub fn mod_values(a: &Value, b: &Value) -> Result<Value, VmError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        let bf = to_f64(b);
        if bf == 0.0 {
            return Err(VmError::DivByZero);
        }
        Ok(Value::Float(to_f64(a) % bf))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        if bi == 0 {
            return Err(VmError::DivByZero);
        }
        // Truncating remainder, matching Rust's `%` operator.
        Ok(Value::Int(ai % bi))
    }
}

pub fn neg_value(v: &Value) -> Result<Value, VmError> {
    match v {
        Value::Int(i) => {
            let res = i.checked_neg().ok_or(VmError::ArithmeticOverflow)?;
            Ok(Value::Int(res))
        }
        Value::Float(f) => Ok(Value::Float(-f)),
        other => Err(VmError::TypeError {
            expected: "numeric",
            got: other.type_name(),
        }),
    }
}

/// Compare two numeric values and return a boolean `Value`.
pub fn compare_values<F>(a: &Value, b: &Value, cmp: F) -> Result<Value, VmError>
where
    F: FnOnce(f64, f64) -> bool,
{
    require_numeric(a, b)?;
    Ok(Value::Bool(cmp(to_f64(a), to_f64(b))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_add_checked() {
        assert_eq!(add_values(&Value::Int(2), &Value::Int(3)).unwrap(), Value::Int(5));
    }

    #[test]
    fn int_add_overflow_errors() {
        let a = Value::Int(i64::MAX);
        let b = Value::Int(1);
        assert!(matches!(add_values(&a, &b), Err(VmError::ArithmeticOverflow)));
    }

    #[test]
    fn mixed_add_promotes_to_float() {
        let r = add_values(&Value::Int(2), &Value::Float(3.5)).unwrap();
        assert_eq!(r, Value::Float(5.5));
    }

    #[test]
    fn string_concat() {
        let r = add_values(&Value::Str("a".into()), &Value::Int(5)).unwrap();
        assert_eq!(r, Value::Str("a5".into()));
    }

    #[test]
    fn div_by_zero_errors() {
        assert!(matches!(div_values(&Value::Int(1), &Value::Int(0)), Err(VmError::DivByZero)));
        assert!(matches!(div_values(&Value::Float(1.0), &Value::Float(0.0)), Err(VmError::DivByZero)));
    }

    #[test]
    fn int_mod_truncates_toward_zero() {
        assert_eq!(mod_values(&Value::Int(7), &Value::Int(3)).unwrap(), Value::Int(1));
        assert_eq!(mod_values(&Value::Int(-7), &Value::Int(3)).unwrap(), Value::Int(-1));
    }

    #[test]
    fn neg_min_int_overflows() {
        assert!(matches!(neg_value(&Value::Int(i64::MIN)), Err(VmError::ArithmeticOverflow)));
    }

    #[test]
    fn compare_mixed_types() {
        assert_eq!(compare_values(&Value::Int(2), &Value::Float(3.0), |a, b| a < b).unwrap(), Value::Bool(true));
    }
}
