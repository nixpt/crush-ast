//! Python-style sequence slicing (`seq.slice`): `s[start:stop:step]` on
//! strings (by character) and arrays. One implementation for the VMs; the AOT
//! runtimes mirror [`slice_indices`] exactly.

use crate::vm::{Value, VmError};

/// The indices `seq[start:stop:step]` selects in a sequence of `len`
/// elements — CPython's `slice.indices()`: negative bounds count from the
/// end, out-of-range bounds clamp, `None` means "from the edge" in the
/// step's direction.
pub fn slice_indices(len: i64, start: Option<i64>, stop: Option<i64>, step: i64) -> Vec<i64> {
    let norm = |v: i64, lo: i64, hi: i64| {
        let v = if v < 0 { v + len } else { v };
        v.clamp(lo, hi)
    };
    let (start, stop) = if step > 0 {
        (start.map_or(0, |v| norm(v, 0, len)), stop.map_or(len, |v| norm(v, 0, len)))
    } else {
        (start.map_or(len - 1, |v| norm(v, -1, len - 1)), stop.map_or(-1, |v| norm(v, -1, len - 1)))
    };
    let mut out = Vec::new();
    let mut i = start;
    while (step > 0 && i < stop) || (step < 0 && i > stop) {
        out.push(i);
        i += step;
    }
    out
}

fn bound(v: &Value, what: &'static str) -> Result<Option<i64>, VmError> {
    match v {
        Value::Null => Ok(None),
        Value::Int(i) => Ok(Some(*i)),
        other => Err(VmError::TypeError { expected: what, got: other.type_name() }),
    }
}

/// `target[start:stop:step]` for a string or an array; bounds and step may be null.
pub fn slice_values(target: &Value, start: &Value, stop: &Value, step: &Value) -> Result<Value, VmError> {
    let step = bound(step, "int or null step")?.unwrap_or(1);
    if step == 0 {
        return Err(VmError::TypeError { expected: "non-zero slice step", got: "0" });
    }
    let (start, stop) = (bound(start, "int or null start")?, bound(stop, "int or null stop")?);
    match target {
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let idx = slice_indices(chars.len() as i64, start, stop, step);
            Ok(Value::Str(idx.into_iter().map(|i| chars[i as usize]).collect()))
        }
        Value::Array(a) => {
            let a = a.borrow();
            let idx = slice_indices(a.len() as i64, start, stop, step);
            Ok(Value::new_array(idx.into_iter().map(|i| a[i as usize].clone()).collect()))
        }
        other => Err(VmError::TypeError { expected: "string or array to slice", got: other.type_name() }),
    }
}

/// `arr_slice(array, start[, end])` — Crush's own `a[start:end]`: the bounds
/// of [`slice_indices`] with step 1, so a negative bound counts from the end
/// as `a[-1]` does (#254: both VMs used to clamp it to 0 — `a[1:-1]` was `[]`
/// and `a[-3:]` the whole array). One implementation for both VMs.
pub fn arr_slice_values(cap: &str, args: &[Value]) -> Result<Value, VmError> {
    if args.len() < 2 {
        return Err(VmError::CapArity { cap: cap.to_string(), expected: 2, got: args.len() });
    }
    let Value::Array(elems) = &args[0] else {
        return Err(VmError::TypeError { expected: "array", got: args[0].type_name() });
    };
    let bound = |v: &Value| match v {
        Value::Int(i) => Ok(Some(*i)),
        Value::Null => Ok(None),
        other => Err(VmError::TypeError { expected: "int or null", got: other.type_name() }),
    };
    let start = bound(&args[1])?;
    let end = match args.get(2) {
        Some(v) => bound(v)?,
        None => None,
    };
    let arr = elems.borrow();
    let idx = slice_indices(arr.len() as i64, start, end, 1);
    Ok(Value::new_array(idx.into_iter().map(|i| arr[i as usize].clone()).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value { Value::Str(v.to_string()) }

    // #254: Crush's `a[start:end]` counts negative bounds from the end.
    #[test]
    fn arr_slice_negative_bounds_count_from_the_end() {
        let n = Value::Null;
        let i = Value::Int;
        let a = || Value::new_array(vec![i(1), i(2), i(3), i(4)]);
        let arr = |v: &[i64]| Value::new_array(v.iter().map(|x| i(*x)).collect());
        let sl = |args: &[Value]| arr_slice_values("arr_slice", args).unwrap();
        assert_eq!(sl(&[a(), i(1), i(-1)]), arr(&[2, 3]));
        assert_eq!(sl(&[a(), i(-3), n.clone()]), arr(&[2, 3, 4]));
        assert_eq!(sl(&[a(), n.clone(), i(-1)]), arr(&[1, 2, 3]));
        assert_eq!(sl(&[a(), i(-3)]), arr(&[2, 3, 4]));
        // non-negative bounds, out-of-range clamping and end <= start: unchanged
        assert_eq!(sl(&[a(), i(1), i(3)]), arr(&[2, 3]));
        assert_eq!(sl(&[a(), i(-100), i(100)]), arr(&[1, 2, 3, 4]));
        assert_eq!(sl(&[a(), i(3), i(1)]), arr(&[]));
        assert!(arr_slice_values("arr_slice", &[a(), Value::Str("x".into())]).is_err());
        assert!(arr_slice_values("arr_slice", &[Value::Str("abc".into()), i(1)]).is_err());
    }

    #[test]
    fn matches_cpython_slices() {
        let n = Value::Null;
        let i = Value::Int;
        assert_eq!(slice_values(&s("crush"), &i(1), &i(3), &n).unwrap(), s("ru"));
        assert_eq!(slice_values(&s("crush"), &i(-2), &n, &n).unwrap(), s("sh"));
        assert_eq!(slice_values(&s("crush"), &n, &n, &i(-1)).unwrap(), s("hsurc"));
        assert_eq!(slice_values(&s("crush"), &n, &n, &i(2)).unwrap(), s("cuh"));
        assert_eq!(slice_values(&s("crush"), &i(10), &n, &n).unwrap(), s(""));
        assert_eq!(slice_values(&s("crush"), &i(-100), &i(2), &n).unwrap(), s("cr"));
        assert_eq!(slice_values(&s("crush"), &i(3), &i(0), &i(-1)).unwrap(), s("sur"));
        let arr = Value::new_array(vec![i(1), i(2), i(3), i(4)]);
        assert_eq!(slice_values(&arr, &i(1), &i(-1), &n).unwrap(), Value::new_array(vec![i(2), i(3)]));
        assert!(slice_values(&s("x"), &n, &n, &i(0)).is_err());
    }
}
