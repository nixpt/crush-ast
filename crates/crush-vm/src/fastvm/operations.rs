//! Helper operations for FastVM execution.

use super::types::{FastError, FastFrame};
use crate::value::RuntimeValue;

/// Check if a value is truthy
#[inline(always)]
pub fn is_truthy(val: &RuntimeValue) -> bool {
    match val {
        RuntimeValue::Bool(b) => *b,
        RuntimeValue::Int(i) => *i != 0,
        RuntimeValue::Null => false,
        _ => true,
    }
}

/// Execute a binary operation on two values
#[inline(always)]
pub fn binary_op<F, G>(
    stack: &mut Vec<RuntimeValue>,
    int_op: F,
    float_op: G,
) -> Result<(), FastError>
where
    F: Fn(i64, i64) -> i64,
    G: Fn(f64, f64) -> f64,
{
    let b = stack.pop().ok_or(FastError::StackUnderflow)?;
    let a = stack.pop().ok_or(FastError::StackUnderflow)?;

    match (&a, &b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => {
            stack.push(RuntimeValue::Int(int_op(*x, *y)));
        }
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => {
            stack.push(RuntimeValue::Float(float_op(*x, *y)));
        }
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => {
            stack.push(RuntimeValue::Float(float_op(*x as f64, *y)));
        }
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => {
            stack.push(RuntimeValue::Float(float_op(*x, *y as f64)));
        }
        _ => return Err(FastError::TypeMismatch),
    }
    Ok(())
}

/// Execute a comparison operation on two values
#[inline(always)]
pub fn compare_op<F, G>(
    stack: &mut Vec<RuntimeValue>,
    int_cmp: F,
    float_cmp: G,
) -> Result<(), FastError>
where
    F: Fn(i64, i64) -> bool,
    G: Fn(f64, f64) -> bool,
{
    let b = stack.pop().ok_or(FastError::StackUnderflow)?;
    let a = stack.pop().ok_or(FastError::StackUnderflow)?;

    let result = match (&a, &b) {
        (RuntimeValue::Int(x), RuntimeValue::Int(y)) => int_cmp(*x, *y),
        (RuntimeValue::Float(x), RuntimeValue::Float(y)) => float_cmp(*x, *y),
        (RuntimeValue::Int(x), RuntimeValue::Float(y)) => float_cmp(*x as f64, *y),
        (RuntimeValue::Float(x), RuntimeValue::Int(y)) => float_cmp(*x, *y as f64),
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::Null, RuntimeValue::Null) => true,
        _ => false,
    };

    stack.push(RuntimeValue::Bool(result));
    Ok(())
}

/// Helper to get current locals base
#[inline(always)]
pub fn current_locals_base(call_stack: &[FastFrame]) -> usize {
    call_stack.last().map(|f| f.locals_base).unwrap_or(0)
}
