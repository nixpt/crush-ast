//! Canonical arithmetic for the FastVM `RuntimeValue` type.
//!
//! This mirrors `crate::arithmetic` but operates on `RuntimeValue` and
//! returns `FastError` so the fast path does not need to convert back and
//! forth to `Value`.

use super::types::FastError;
use crate::memory::{Arena, Object};
use crate::value::RuntimeValue;

#[inline]
fn is_float(v: &RuntimeValue) -> bool {
    matches!(v, RuntimeValue::Float(_))
}

fn is_string(v: &RuntimeValue, arena: &Arena) -> bool {
    matches!(v, RuntimeValue::String(_))
        || matches!(v, RuntimeValue::Ref(ptr) if matches!(arena.get(*ptr), Some(Object::Str(_))))
}

#[inline]
fn to_f64(v: &RuntimeValue) -> f64 {
    match v {
        RuntimeValue::Int(i) => *i as f64,
        RuntimeValue::Float(f) => *f,
        _ => 0.0,
    }
}

#[inline]
fn to_i64(v: &RuntimeValue) -> i64 {
    match v {
        RuntimeValue::Int(i) => *i,
        RuntimeValue::Float(f) => *f as i64,
        _ => 0,
    }
}

fn require_numeric(a: &RuntimeValue, b: &RuntimeValue) -> Result<(), FastError> {
    if !matches!(a, RuntimeValue::Int(_) | RuntimeValue::Float(_)) {
        return Err(FastError::TypeMismatch);
    }
    if !matches!(b, RuntimeValue::Int(_) | RuntimeValue::Float(_)) {
        return Err(FastError::TypeMismatch);
    }
    Ok(())
}

/// ADD with string concatenation when either side is a string.
pub fn add_rtv(a: &RuntimeValue, b: &RuntimeValue, arena: &Arena) -> Result<RuntimeValue, FastError> {
    if is_string(a, arena) || is_string(b, arena) {
        let s = format!("{}{}", rtv_as_text(a, arena), rtv_as_text(b, arena));
        return Ok(RuntimeValue::String(s));
    }
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        Ok(RuntimeValue::Float(to_f64(a) + to_f64(b)))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        let res = ai.checked_add(bi).ok_or(FastError::ArithmeticOverflow)?;
        Ok(RuntimeValue::Int(res))
    }
}

pub fn sub_rtv(a: &RuntimeValue, b: &RuntimeValue) -> Result<RuntimeValue, FastError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        Ok(RuntimeValue::Float(to_f64(a) - to_f64(b)))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        let res = ai.checked_sub(bi).ok_or(FastError::ArithmeticOverflow)?;
        Ok(RuntimeValue::Int(res))
    }
}

pub fn mul_rtv(a: &RuntimeValue, b: &RuntimeValue) -> Result<RuntimeValue, FastError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        Ok(RuntimeValue::Float(to_f64(a) * to_f64(b)))
    } else {
        let ai = to_i64(a);
        let bi = to_i64(b);
        let res = ai.checked_mul(bi).ok_or(FastError::ArithmeticOverflow)?;
        Ok(RuntimeValue::Int(res))
    }
}

pub fn div_rtv(a: &RuntimeValue, b: &RuntimeValue) -> Result<RuntimeValue, FastError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        let bf = to_f64(b);
        if bf == 0.0 {
            return Err(FastError::DivisionByZero);
        }
        Ok(RuntimeValue::Float(to_f64(a) / bf))
    } else {
        let bi = to_i64(b);
        if bi == 0 {
            return Err(FastError::DivisionByZero);
        }
        Ok(RuntimeValue::Int(to_i64(a) / bi))
    }
}

pub fn mod_rtv(a: &RuntimeValue, b: &RuntimeValue) -> Result<RuntimeValue, FastError> {
    require_numeric(a, b)?;
    if is_float(a) || is_float(b) {
        let bf = to_f64(b);
        if bf == 0.0 {
            return Err(FastError::DivisionByZero);
        }
        Ok(RuntimeValue::Float(to_f64(a) % bf))
    } else {
        let bi = to_i64(b);
        if bi == 0 {
            return Err(FastError::DivisionByZero);
        }
        Ok(RuntimeValue::Int(to_i64(a) % bi))
    }
}

pub fn neg_rtv(v: &RuntimeValue) -> Result<RuntimeValue, FastError> {
    match v {
        RuntimeValue::Int(i) => {
            let res = i.checked_neg().ok_or(FastError::ArithmeticOverflow)?;
            Ok(RuntimeValue::Int(res))
        }
        RuntimeValue::Float(f) => Ok(RuntimeValue::Float(-*f)),
        _ => Err(FastError::TypeMismatch),
    }
}

pub fn compare_rtv<F>(a: &RuntimeValue, b: &RuntimeValue, cmp: F) -> Result<RuntimeValue, FastError>
where
    F: FnOnce(f64, f64) -> bool,
{
    require_numeric(a, b)?;
    Ok(RuntimeValue::Bool(cmp(to_f64(a), to_f64(b))))
}

fn rtv_as_text(v: &RuntimeValue, arena: &Arena) -> String {
    match v {
        RuntimeValue::String(s) => s.clone(),
        RuntimeValue::Int(i) => i.to_string(),
        RuntimeValue::Float(f) => f.to_string(),
        RuntimeValue::Bool(b) => b.to_string(),
        RuntimeValue::Null => "null".to_string(),
        RuntimeValue::Ref(idx) => match arena.get(*idx) {
            Some(Object::Str(s)) => s.clone(),
            _ => format!("@{idx}"),
        },
    }
}
