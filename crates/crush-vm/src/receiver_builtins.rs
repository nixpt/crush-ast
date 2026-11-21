//! Shared pure array/container receiver operations for `CAP_CALL` and
//! `CALL_MEMBER`'s §9.4 built-in fallback (GitHub #559).
//!
//! One implementation, called by both CVM1 engines (`scheduler.rs` and
//! `portable_vm.rs`) from both `dispatch_cap` and `CALL_MEMBER` so the
//! capability path and the member-call path cannot drift.
//!
//! Maps NEVER enter the `CALL_MEMBER` built-in path: `cosm_rt::call_member`
//! handles `Value::Map` by field lookup only (§9.4). The `Value::Map` arms in
//! `call_cap("arr_get" | "arr_set")` exist solely for the function/subscript
//! capability forms (`arr_get(m, k)`, `m[k] = v`).

use crate::scheduler::{array_get, need_array_index, wrap_index};
use crate::vm::{Value, VmError};

/// Built-in member names supported on a `Value::Array` receiver through
/// `CALL_MEMBER` (§9.4).
pub const ARRAY_MEMBER_NAMES: &[&str] = &[
    "append",
    "push",
    "pop",
    "arr_get",
    "arr_set",
    "arr_slice",
];

/// Whether `name` is a built-in array member operation for `CALL_MEMBER`.
pub fn is_array_member(name: &str) -> bool {
    ARRAY_MEMBER_NAMES.contains(&name)
}

fn array_set(arr: &mut [Value], idx_v: &Value, val: Value) -> Result<(), VmError> {
    let idx = need_array_index(idx_v)?;
    let actual = wrap_index(idx, arr.len())?;
    arr[actual] = val;
    Ok(())
}

/// Run a built-in array member call (`CALL_MEMBER` on `Value::Array`).
///
/// `args` are the `argc` non-receiver arguments in call order.
pub fn call_array_member(recv: &Value, name: &str, args: &[Value]) -> Result<Value, VmError> {
    let Value::Array(elems) = recv else {
        return Err(VmError::TypeError {
            expected: "array",
            got: recv.type_name(),
        });
    };
    let arity_err = |expected: usize| {
        Err(VmError::FuncArity {
            function: format!("array.{name}"),
            expected,
            got: args.len(),
        })
    };
    match name {
        "append" | "push" => {
            if args.len() != 1 {
                return arity_err(1);
            }
            elems.borrow_mut().push(args[0].clone());
            Ok(recv.clone())
        }
        "pop" => {
            if !args.is_empty() {
                return arity_err(0);
            }
            Ok(elems.borrow_mut().pop().unwrap_or(Value::Null))
        }
        "arr_get" => {
            if args.len() != 1 {
                return arity_err(1);
            }
            array_get(&elems.borrow(), &args[0])
        }
        "arr_set" => {
            if args.len() != 2 {
                return arity_err(2);
            }
            array_set(&mut elems.borrow_mut(), &args[0], args[1].clone())?;
            Ok(recv.clone())
        }
        "arr_slice" => {
            if args.is_empty() || args.len() > 2 {
                return arity_err(2);
            }
            let mut full = Vec::with_capacity(1 + args.len());
            full.push(recv.clone());
            full.extend_from_slice(args);
            crate::seq::arr_slice_values("arr_slice", &full)
        }
        _ => Err(VmError::Cosm(format!(
            "`.{name}(…)` on an array: member calls resolve on struct instances (COSM Records) and maps"
        ))),
    }
}

/// Run one of the pure array/container capabilities from `dispatch_cap`
/// (`args[0]` is the receiver/container).
pub fn call_cap(cap: &str, args: &[Value]) -> Result<Value, VmError> {
    match cap {
        "arr_slice" => crate::seq::arr_slice_values(cap, args),
        "append" | "push" => {
            if args.len() < 2 {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected: 2,
                    got: args.len(),
                });
            }
            match &args[0] {
                Value::Array(elems) => {
                    elems.borrow_mut().push(args[1].clone());
                    Ok(args[0].clone())
                }
                _ => Err(VmError::TypeError {
                    expected: "array",
                    got: args[0].type_name(),
                }),
            }
        }
        "pop" => {
            // Same contract on both VMs: the removed element (null when empty),
            // the method-call form's value (CRUSH-391).
            if args.len() != 1 {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            match &args[0] {
                Value::Array(elems) => Ok(elems.borrow_mut().pop().unwrap_or(Value::Null)),
                _ => Err(VmError::TypeError {
                    expected: "array",
                    got: args[0].type_name(),
                }),
            }
        }
        "arr_set" => {
            if args.len() < 3 {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected: 3,
                    got: args.len(),
                });
            }
            match &args[0] {
                Value::Array(elems) => {
                    // Same contract as the ARR_SET opcode (#442): a negative
                    // index counts from the end, out of range is a bounds error
                    // and a non-Int index is BadIndex.
                    array_set(&mut elems.borrow_mut(), &args[1], args[2].clone())?;
                    Ok(args[0].clone())
                }
                Value::Map(map) => {
                    let key = crate::map_ops::map_key_string(&args[1])?;
                    map.borrow_mut().insert(key, args[2].clone());
                    Ok(args[0].clone())
                }
                _ => Err(VmError::TypeError {
                    expected: "array or map",
                    got: args[0].type_name(),
                }),
            }
        }
        "arr_get" => {
            if args.len() < 2 {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected: 2,
                    got: args.len(),
                });
            }
            match &args[0] {
                // #446: the `index` op's contract, through the same helper.
                Value::Array(elems) => array_get(&elems.borrow(), &args[1]),
                Value::Map(map) => {
                    let key = crate::map_ops::map_key_string(&args[1])?;
                    crate::map_ops::map_get(&map.borrow(), &key)
                }
                _ => Err(VmError::TypeError {
                    expected: "array or map",
                    got: args[0].type_name(),
                }),
            }
        }
        _ => Err(VmError::UnknownCap(cap.to_string())),
    }
}
