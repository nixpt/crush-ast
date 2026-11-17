//! The built-in receiver calls: `xs.push(v)`, `xs.pop()`, `xs.arr_get(i)`, …
//!
//! One implementation for both CVM1 engines, reached two ways:
//!
//! - the capability dispatch (`cap_call "push"` and friends), which is what
//!   `xs.push(v)` on a declared variable lowers to when no struct declares a
//!   method of that name;
//! - `CALL_MEMBER` on a receiver that is not a Record, a Kind or a map
//!   ([`crate::cosm_rt::call_member`], `docs/design/object-model.md` §9.4):
//!   a struct method named `push` makes the compiler lower every `x.push(…)`
//!   to `CALL_MEMBER`, and a computed receiver (`self.a.push(1)`) always is.
//!   Before #435 those calls failed on an array.
//!
//! `args[0]` is the receiver; the rest are the call's arguments in order.
//! Every row is an exact, in-process operation on the receiver: none needs a
//! host capability or calls back into user code (`parallel_map` does, so it
//! is not a row; see [`crate::cosm_rt::call_member`]).

use crate::vm::{Value, VmError};

/// The names [`call`] implements.
pub const NAMES: &[&str] = &[
    "push",
    "append",
    "pop",
    "arr_get",
    "arr_set",
    "arr_slice",
    "make_range",
];

/// Whether `name` is a built-in receiver call.
pub fn is_builtin(name: &str) -> bool {
    NAMES.contains(&name)
}

/// Run the built-in receiver call `name` with `args` (the receiver first).
///
/// The arity rule is the capability registry's (`crate::caps`), checked
/// here as well as in the capability dispatch, so a call is refused the same
/// way on both routes: an exact count for every row but `make_range`, whose
/// 1 to 3 arguments [`crate::make_range::make_range_values`] checks.
pub fn call(name: &str, args: &[Value]) -> Result<Value, VmError> {
    if let Some(expected) = crate::caps::capabilities().get(name).and_then(|s| s.argc)
        && args.len() != expected
    {
        return Err(VmError::CapArity {
            cap: name.to_string(),
            expected,
            got: args.len(),
        });
    }
    match name {
        "arr_slice" => crate::seq::arr_slice_values(name, args),
        "make_range" => crate::make_range::make_range_values(args),
        "append" | "push" => match &args[0] {
            Value::Array(elems) => {
                elems.borrow_mut().push(args[1].clone());
                Ok(args[0].clone())
            }
            _ => Err(VmError::TypeError {
                expected: "array",
                got: args[0].type_name(),
            }),
        },
        // The removed element (null when empty), the method-call form's value
        // (CRUSH-391) — not the array: `let x = a.pop()` wants the element.
        "pop" => match &args[0] {
            Value::Array(elems) => Ok(elems.borrow_mut().pop().unwrap_or(Value::Null)),
            _ => Err(VmError::TypeError {
                expected: "array",
                got: args[0].type_name(),
            }),
        },
        "arr_set" => {
            match &args[0] {
                Value::Array(elems) => {
                    // Same contract as the ARR_SET opcode (#442): a negative
                    // index counts from the end, out of range is a bounds
                    // error and a non-Int index is BadIndex.
                    let idx = crate::scheduler::need_array_index(&args[1])?;
                    let mut arr = elems.borrow_mut();
                    let actual = crate::scheduler::wrap_index(idx, arr.len())?;
                    arr[actual] = args[2].clone();
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
            match &args[0] {
                // #446: the `index` op's contract, through the same helper.
                Value::Array(elems) => crate::scheduler::array_get(&elems.borrow(), &args[1]),
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
        _ => Err(VmError::UnknownCap(name.to_string())),
    }
}
