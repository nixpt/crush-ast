//! Shared `make_range` implementation for every VM backend.
//!
//! Supports the Python/`range`-shaped forms:
//! - `make_range(stop)` → `[0, stop)` step 1
//! - `make_range(start, stop)` → `[start, stop)` step 1
//! - `make_range(start, stop, step)` → stepped sequence (step may be negative)
//!
//! Empty / zero-arg defaults are intentionally not provided — callers must
//! pass integers. A zero step is a type/value error.

use crate::vm::{Value, VmError};

fn need_int(v: &Value, what: &'static str) -> Result<i64, VmError> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(VmError::TypeError {
            expected: what,
            got: other.type_name(),
        }),
    }
}

/// Build an integer range array from 1–3 integer arguments.
pub fn make_range_values(args: &[Value]) -> Result<Value, VmError> {
    let (start, stop, step) = match args {
        [stop] => (0i64, need_int(stop, "int stop")?, 1i64),
        [start, stop] => (
            need_int(start, "int start")?,
            need_int(stop, "int stop")?,
            1i64,
        ),
        [start, stop, step] => (
            need_int(start, "int start")?,
            need_int(stop, "int stop")?,
            need_int(step, "int step")?,
        ),
        [] => {
            return Err(VmError::CapArity {
                cap: "make_range".to_string(),
                expected: 1,
                got: 0,
            });
        }
        _ => {
            return Err(VmError::CapArity {
                cap: "make_range".to_string(),
                expected: 3,
                got: args.len(),
            });
        }
    };

    if step == 0 {
        return Err(VmError::TypeError {
            expected: "non-zero step",
            got: "0",
        });
    }

    let mut elems = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < stop {
            elems.push(Value::Int(i));
            // Python `range` just ends when the next step would overflow;
            // do not surface ArithmeticOverflow.
            match i.checked_add(step) {
                Some(next) => i = next,
                None => break,
            }
        }
    } else {
        let mut i = start;
        while i > stop {
            elems.push(Value::Int(i));
            match i.checked_add(step) {
                Some(next) => i = next,
                None => break,
            }
        }
    }
    Ok(Value::new_array(elems))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_only() {
        let v = make_range_values(&[Value::Int(3)]).unwrap();
        let Value::Array(a) = v else { panic!() };
        assert_eq!(
            a.borrow().as_slice(),
            &[Value::Int(0), Value::Int(1), Value::Int(2)]
        );
    }

    #[test]
    fn stepped() {
        let v = make_range_values(&[Value::Int(1), Value::Int(10), Value::Int(3)]).unwrap();
        let Value::Array(a) = v else { panic!() };
        assert_eq!(
            a.borrow().as_slice(),
            &[Value::Int(1), Value::Int(4), Value::Int(7)]
        );
    }

    #[test]
    fn negative_step() {
        let v = make_range_values(&[Value::Int(5), Value::Int(0), Value::Int(-2)]).unwrap();
        let Value::Array(a) = v else { panic!() };
        assert_eq!(
            a.borrow().as_slice(),
            &[Value::Int(5), Value::Int(3), Value::Int(1)]
        );
    }

    #[test]
    fn zero_step_errors() {
        assert!(make_range_values(&[Value::Int(0), Value::Int(5), Value::Int(0)]).is_err());
    }
}
