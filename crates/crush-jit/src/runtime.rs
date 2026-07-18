//! Runtime context shared between JIT-compiled code and the host.
//!
//! The [`JitContext`] is passed as a pointer argument to every JIT-compiled function.
//! Field offsets are fixed (ABI-stable) so the compiler can emit known offsets.
//!
//! # Layout (64-bit systems)
//!
//! | Offset | Size | Field |
//! |--------|------|-------|
//! | 0      | 8192 | stack[1024] |
//! | 8192   | 8    | stack_top |
//! | 8200   | 512  | locals[64] |
//! | 8712   | 8    | n_locals |
//! | 8720   | 8    | result |
//! | 8728   | 8    | budget |
//! | 8736   | 4    | error |
//! | 8740   | 4    | _pad |
//! | 8744   | 8    | arena |
//! | 8752   | 8    | capabilities |
//! | 8760   | 8    | hal |
//! | 8768   | 1024 | call_stack[64] |
//! | 9792   | 8    | call_stack_top |
//! | 9800   | 8    | helper_fn (function pointer) |
//! | 9808   | 8    | strings_ptr (raw pointer to Vec<String>) |
//! | 9816   | 8    | handler_pc (resolved handler PC for throw dispatch) |
//! | 9824   | 8    | handler_stack_top |
//! | 9832   | 256  | handler_stack[16] (JitHandlerFrame × 16) |
//! | 10088  | —    | total |

use crate::value::{JitValue, TAG_NULL, TAG_FALSE};
use crush_vm::fastvm::similarity::calculate_similarity;
use crush_vm::memory::{Arena, Object};
use crush_vm::value::RuntimeValue;
use std::ffi::c_void;

/// A no-op HAL used as default when no real HAL is provided to the JIT.
#[doc(hidden)]
#[derive(Debug)]
pub struct DummyHal;
impl crush_vm::fastvm::Hal for DummyHal {}

/// Maximum number of stack slots before spilling.
pub const JIT_STACK_SIZE: usize = 1024;

/// Maximum number of local variables.
pub const JIT_MAX_LOCALS: usize = 64;

/// Maximum call depth before overflow.
pub const JIT_MAX_CALL_DEPTH: usize = 64;

/// Type of a JIT runtime helper function.
///
/// Called by JIT-compiled code via `call_indirect`. Receives the context pointer,
/// an opcode identifying the operation, and an extra `arg` value (typically
/// `instr.arg` from the instruction being compiled).
pub type JitHelper = unsafe extern "C" fn(ctx: *mut JitContext, opcode: i64, arg: i64);

// ── Helper opcodes ─────────────────────────────────────────────────────────

pub(crate) const OP_PUSH_STR: i64 = 0;
pub(crate) const OP_MAKE_LIST: i64 = 1;
pub(crate) const OP_MAKE_MAP: i64 = 2;
pub(crate) const OP_INDEX: i64 = 3;
pub(crate) const OP_LEN: i64 = 4;
pub(crate) const OP_TYPEOF: i64 = 5;
pub(crate) const OP_NEW_ARRAY: i64 = 6;
pub(crate) const OP_ARRAY_PUSH: i64 = 7;
pub(crate) const OP_ARRAY_POP: i64 = 8;
pub(crate) const OP_ARR_SET: i64 = 9;
pub(crate) const OP_STR_CONTAINS: i64 = 10;
pub(crate) const OP_STR_STARTS_WITH: i64 = 11;
pub(crate) const OP_STR_ENDS_WITH: i64 = 12;
pub(crate) const OP_STR_TO_UPPER: i64 = 13;
pub(crate) const OP_STR_TO_LOWER: i64 = 14;
pub(crate) const OP_STR_TRIM: i64 = 15;
pub(crate) const OP_STR_SPLIT: i64 = 16;
pub(crate) const OP_STR_REPLACE: i64 = 17;
pub(crate) const OP_STR_JOIN: i64 = 18;
pub(crate) const OP_CAST: i64 = 19;
pub(crate) const OP_NEW_TUPLE: i64 = 20;
pub(crate) const OP_NEW_LIST: i64 = 21;
pub(crate) const OP_NEW_VECTOR: i64 = 22;
pub(crate) const OP_NEW_SET: i64 = 23;
pub(crate) const OP_MAKE_RANGE: i64 = 24;
pub(crate) const OP_CAP_CALL: i64 = 25;
pub(crate) const OP_TUPLE_PUSH: i64 = 26;
pub(crate) const OP_LIST_PUSH: i64 = 27;
pub(crate) const OP_VECTOR_PUSH: i64 = 28;
pub(crate) const OP_SET_PUSH: i64 = 29;
pub(crate) const OP_GET_FIELD: i64 = 30;
pub(crate) const OP_SET_FIELD: i64 = 31;
pub(crate) const OP_NEW_OBJ: i64 = 32;
pub(crate) const OP_NEW_STRUCT: i64 = 33;
pub(crate) const OP_STR_SIM: i64 = 34;
pub(crate) const OP_ENTER_TRY: i64 = 35;
pub(crate) const OP_EXIT_TRY: i64 = 36;
pub(crate) const OP_THROW: i64 = 37;

/// Default no-op helper (used when no helper is registered).
unsafe extern "C" fn jit_helper_noop(_ctx: *mut JitContext, _opcode: i64, _arg: i64) {}

/// Convert a `JitValue` to a `RuntimeValue`.
fn jit_to_rtv(v: JitValue) -> RuntimeValue {
    if let Some(i) = v.to_int() {
        RuntimeValue::Int(i)
    } else if let Some(f) = v.to_float() {
        RuntimeValue::Float(f)
    } else if let Some(b) = v.to_bool() {
        RuntimeValue::Bool(b)
    } else if v.is_null() {
        RuntimeValue::Null
    } else if let Some(idx) = v.to_ref() {
        RuntimeValue::Ref(idx)
    } else {
        RuntimeValue::Null
    }
}

/// Convert a `&RuntimeValue` to a `JitValue`.
fn rtv_to_jit(v: &RuntimeValue) -> JitValue {
    match v {
        RuntimeValue::Int(i) => JitValue::int(*i),
        RuntimeValue::Float(f) => JitValue::float(*f),
        RuntimeValue::Bool(b) => JitValue::bool(*b),
        RuntimeValue::Null => JitValue::null(),
        RuntimeValue::Ref(idx) => JitValue::from_ref(*idx),
        RuntimeValue::String(s) => {
            // Direct string values should not appear in JIT context;
            // if they do, fall back to null.
            JitValue::null()
        }
    }
}

/// Helper: unwrap arena pointer, returning None if null.
fn arena_mut(ptr: *mut c_void) -> Option<&'static mut Arena> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *(ptr as *mut Arena) })
    }
}

/// Helper: read-only arena access.
fn arena_ref(ptr: *mut c_void) -> Option<&'static Arena> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*(ptr as *const Arena) })
    }
}

/// Helper: convert a JitValue into a String by inspecting arena contents.
fn jit_val_to_string(v: JitValue, arena: &Arena) -> Option<String> {
    if let Some(ref_idx) = v.to_ref() {
        if let Some(Object::Str(s)) = arena.get(ref_idx) {
            return Some(s.clone());
        }
    }
    None
}

/// Helper: convert a Vec<JitValue> to Vec<RuntimeValue>.
fn jit_vec_to_rtv(items: Vec<JitValue>) -> Vec<RuntimeValue> {
    items.into_iter().map(jit_to_rtv).collect()
}

/// Helper: convert &[RuntimeValue] to Vec<JitValue>.
fn rtv_slice_to_jit(items: &[RuntimeValue]) -> Vec<JitValue> {
    items.iter().map(rtv_to_jit).collect()
}

/// JIT runtime helper dispatch function.
///
/// Called from JIT-compiled code for arena-dependent operations. Handles
/// PushStr, MakeList, Index, Len, TypeOf, string ops, array ops, Cast, etc.
///
/// # Safety
///
/// `ctx` must point to a valid, aligned `JitContext`. Its `strings_ptr` must
/// point to a live `Vec<String>` (or be null). Its `arena` must point to a
/// live `Arena` (or be null).
pub unsafe extern "C" fn jit_runtime_helper(ctx: *mut JitContext, opcode: i64, arg: i64) {
    // SAFETY: `ctx` is guaranteed valid by the JIT caller.
    let ctx = unsafe { &mut *ctx };
    let strings: Option<&Vec<String>> = if ctx.strings_ptr.is_null() {
        None
    } else {
        unsafe { Some(&*(ctx.strings_ptr as *const Vec<String>)) }
    };

    match opcode {
        // ════════════════════════════════════════════════════════════════════
        // OP_PUSH_STR (0)
        // ════════════════════════════════════════════════════════════════════
        OP_PUSH_STR => {
            let str_idx = arg as usize;
            let s = strings
                .and_then(|ss| ss.get(str_idx))
                .cloned()
                .unwrap_or_default();
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let ptr = arena.alloc(Object::Str(s));
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_MAKE_LIST (1)
        // ════════════════════════════════════════════════════════════════════
        OP_MAKE_LIST => {
            let count = arg as usize;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(ctx.pop().unwrap_or(JitValue::null()));
            }
            items.reverse();
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let ptr = arena.alloc(Object::Array(jit_vec_to_rtv(items)));
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_MAKE_MAP (2)
        // ════════════════════════════════════════════════════════════════════
        OP_MAKE_MAP => {
            let count = arg as usize;
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let mut map = std::collections::HashMap::new();
            for _ in 0..count {
                let val = ctx.pop().unwrap_or(JitValue::null());
                let key = ctx.pop().unwrap_or(JitValue::null());
                if let Some(s) = jit_val_to_string(key, arena) {
                    map.insert(s, jit_to_rtv(val));
                }
            }
            let ptr = arena.alloc(Object::Map(map));
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_INDEX (3)
        // ════════════════════════════════════════════════════════════════════
        OP_INDEX => {
            let key = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let result = match container.to_ref() {
                Some(ref_idx) => match arena.get(ref_idx) {
                    Some(Object::Array(items)) => {
                        match key.to_int() {
                            Some(idx) => items.get(idx as usize)
                                .map(rtv_to_jit)
                                .unwrap_or(JitValue::null()),
                            None => JitValue::null(),
                        }
                    }
                    Some(Object::Map(map)) => {
                        jit_val_to_string(key, arena)
                            .and_then(|s| map.get(&s))
                            .map(rtv_to_jit)
                            .unwrap_or(JitValue::null())
                    }
                    Some(Object::Str(s)) => {
                        match key.to_int() {
                            Some(idx) => {
                                let char_idx = idx as usize;
                                s.chars().nth(char_idx)
                                    .map(|c| {
                                        if let Some(a) = arena_mut(ctx.arena) {
                                            let cp = a.alloc(Object::Str(c.to_string()));
                                            JitValue::from_ref(cp)
                                        } else {
                                            JitValue::null()
                                        }
                                    })
                                    .unwrap_or(JitValue::null())
                            }
                            None => JitValue::null(),
                        }
                    }
                    _ => JitValue::null(),
                },
                None => JitValue::null(),
            };
            ctx.push(result);
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_LEN (4)
        // ════════════════════════════════════════════════════════════════════
        OP_LEN => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let len = match val.to_ref() {
                Some(ref_idx) => match arena.get(ref_idx) {
                    Some(Object::Array(a)) => a.len(),
                    Some(Object::Map(m)) => m.len(),
                    Some(Object::Str(s)) => s.len(),
                    Some(Object::Tuple(t)) => t.len(),
                    Some(Object::Vector(v)) => v.len(),
                    Some(Object::Set(s)) => s.len(),
                    _ => { ctx.push(JitValue::null()); return; }
                },
                None => { ctx.push(JitValue::null()); return; }
            };
            ctx.push(JitValue::int(len as i64));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_TYPEOF (5)
        // ════════════════════════════════════════════════════════════════════
        OP_TYPEOF => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let type_name = if val.is_null() {
                "null"
            } else if val.to_bool().is_some() {
                "bool"
            } else if val.is_int() {
                "int"
            } else if val.is_float() {
                "float"
            } else if let Some(ref_idx) = val.to_ref() {
                match arena.get(ref_idx) {
                    Some(Object::Str(_)) => "string",
                    Some(Object::Array(_)) => "array",
                    Some(Object::Map(_)) => "map",
                    Some(Object::Tuple(_)) => "tuple",
                    Some(Object::List(_)) => "list",
                    Some(Object::Vector(_)) => "vector",
                    Some(Object::Set(_)) => "set",
                    Some(_) => "ref",
                    None => "unknown",
                }
            } else {
                "unknown"
            };
            let ptr = arena.alloc(Object::Str(type_name.to_string()));
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_NEW_ARRAY (6)
        // ════════════════════════════════════════════════════════════════════
        OP_NEW_ARRAY => {
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let ptr = arena.alloc(Object::Array(Vec::new()));
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_ARRAY_PUSH (7)
        // ════════════════════════════════════════════════════════════════════
        OP_ARRAY_PUSH => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = container.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Array(arr)) = arena.get_mut(ref_idx) {
                        arr.push(jit_to_rtv(val));
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_ARRAY_POP (8)
        // ════════════════════════════════════════════════════════════════════
        OP_ARRAY_POP => {
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = container.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Array(arr)) = arena.get_mut(ref_idx) {
                        let result = arr.pop().map(|v| rtv_to_jit(&v))
                            .unwrap_or(JitValue::null());
                        ctx.push(result);
                        return;
                    }
                }
            }
            ctx.push(JitValue::null());
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_ARR_SET (9)
        // ════════════════════════════════════════════════════════════════════
        OP_ARR_SET => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let key = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let (Some(ref_idx), Some(idx)) = (container.to_ref(), key.to_int()) {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Array(arr)) = arena.get_mut(ref_idx) {
                        let i = idx as usize;
                        if i < arr.len() {
                            arr[i] = jit_to_rtv(val);
                        }
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_CONTAINS (10)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_CONTAINS => {
            let pattern = ctx.pop().unwrap_or(JitValue::null());
            let target = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (jit_val_to_string(pattern, &arena), jit_val_to_string(target, &arena)) {
                (Some(p), Some(s)) => ctx.push(JitValue::bool(s.contains(&p))),
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_STARTS_WITH (11)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_STARTS_WITH => {
            let pattern = ctx.pop().unwrap_or(JitValue::null());
            let target = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (jit_val_to_string(pattern, &arena), jit_val_to_string(target, &arena)) {
                (Some(p), Some(s)) => ctx.push(JitValue::bool(s.starts_with(&p))),
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_ENDS_WITH (12)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_ENDS_WITH => {
            let pattern = ctx.pop().unwrap_or(JitValue::null());
            let target = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (jit_val_to_string(pattern, &arena), jit_val_to_string(target, &arena)) {
                (Some(p), Some(s)) => ctx.push(JitValue::bool(s.ends_with(&p))),
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_TO_UPPER (13)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_TO_UPPER => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match jit_val_to_string(val, arena) {
                Some(s) => {
                    let ptr = arena.alloc(Object::Str(s.to_uppercase()));
                    ctx.push(JitValue::from_ref(ptr));
                }
                None => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_TO_LOWER (14)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_TO_LOWER => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match jit_val_to_string(val, arena) {
                Some(s) => {
                    let ptr = arena.alloc(Object::Str(s.to_lowercase()));
                    ctx.push(JitValue::from_ref(ptr));
                }
                None => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_TRIM (15)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_TRIM => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match jit_val_to_string(val, arena) {
                Some(s) => {
                    let ptr = arena.alloc(Object::Str(s.trim().to_string()));
                    ctx.push(JitValue::from_ref(ptr));
                }
                None => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_SPLIT (16)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_SPLIT => {
            let delim = ctx.pop().unwrap_or(JitValue::null());
            let target = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (jit_val_to_string(delim, arena), jit_val_to_string(target, arena)) {
                (Some(d), Some(s)) => {
                    let parts: Vec<RuntimeValue> = s.split(&d)
                        .map(|p| arena.alloc(Object::Str(p.to_string())))
                        .map(|idx| RuntimeValue::Ref(idx))
                        .collect();
                    let arr = arena.alloc(Object::Array(parts));
                    ctx.push(JitValue::from_ref(arr));
                }
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_REPLACE (17)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_REPLACE => {
            let new_val = ctx.pop().unwrap_or(JitValue::null());
            let old_val = ctx.pop().unwrap_or(JitValue::null());
            let target = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (
                jit_val_to_string(new_val, arena),
                jit_val_to_string(old_val, arena),
                jit_val_to_string(target, arena),
            ) {
                (Some(n), Some(o), Some(s)) => {
                    let replaced = s.replace(&o, &n);
                    let ptr = arena.alloc(Object::Str(replaced));
                    ctx.push(JitValue::from_ref(ptr));
                }
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_JOIN (18)
        // ════════════════════════════════════════════════════════════════════
        OP_STR_JOIN => {
            let delim = ctx.pop().unwrap_or(JitValue::null());
            let arr_val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (jit_val_to_string(delim, arena), arr_val.to_ref()) {
                (Some(d), Some(arr_ref)) => {
                    match arena.get(arr_ref) {
                        Some(Object::Array(arr)) => {
                            let mut strings = Vec::new();
                            for val in arr {
                                if let RuntimeValue::Ref(sp) = val {
                                    if let Some(Object::Str(s)) = arena.get(*sp) {
                                        strings.push(s.clone());
                                    } else {
                                        ctx.push(JitValue::null());
                                        return;
                                    }
                                } else {
                                    ctx.push(JitValue::null());
                                    return;
                                }
                            }
                            let joined = strings.join(&d);
                            let res = arena.alloc(Object::Str(joined));
                            ctx.push(JitValue::from_ref(res));
                        }
                        _ => ctx.push(JitValue::null()),
                    }
                }
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_CAST (19)
        // ════════════════════════════════════════════════════════════════════
        OP_CAST => {
            let type_idx = arg as usize;
            let val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let type_name = strings
                .and_then(|ss| ss.get(type_idx))
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            let result = match type_name {
                "int" => {
                    if let Some(v) = val.to_int() {
                        JitValue::int(v)
                    } else if let Some(v) = val.to_float() {
                        JitValue::int(v as i64)
                    } else if let Some(b) = val.to_bool() {
                        JitValue::int(if b { 1 } else { 0 })
                    } else if let Some(ref_idx) = val.to_ref() {
                        if let Some(Object::Str(s)) = arena.get(ref_idx) {
                            JitValue::int(s.parse::<i64>().unwrap_or(0))
                        } else {
                            JitValue::null()
                        }
                    } else {
                        JitValue::null()
                    }
                }
                "float" => {
                    if let Some(v) = val.to_float() {
                        JitValue::float(v)
                    } else if let Some(v) = val.to_int() {
                        JitValue::float(v as f64)
                    } else if let Some(b) = val.to_bool() {
                        JitValue::float(if b { 1.0 } else { 0.0 })
                    } else if let Some(ref_idx) = val.to_ref() {
                        if let Some(Object::Str(s)) = arena.get(ref_idx) {
                            JitValue::float(s.parse::<f64>().unwrap_or(0.0))
                        } else {
                            JitValue::null()
                        }
                    } else {
                        JitValue::null()
                    }
                }
                "string" => {
                    let s = if let Some(v) = val.to_int() {
                        v.to_string()
                    } else if let Some(v) = val.to_float() {
                        v.to_string()
                    } else if let Some(b) = val.to_bool() {
                        b.to_string()
                    } else if val.is_null() {
                        "null".to_string()
                    } else if let Some(ref_idx) = val.to_ref() {
                        if let Some(Object::Str(s)) = arena.get(ref_idx) {
                            s.clone()
                        } else {
                            ctx.push(JitValue::null());
                            return;
                        }
                    } else {
                        ctx.push(JitValue::null());
                        return;
                    };
                    let ptr = arena.alloc(Object::Str(s));
                    JitValue::from_ref(ptr)
                }
                "bool" => {
                    JitValue::bool(!val.is_null() && val.0 != TAG_FALSE)
                }
                _ => JitValue::null(),
            };
            ctx.push(result);
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_NEW_TUPLE / LIST / VECTOR / SET (20-23)
        // ════════════════════════════════════════════════════════════════════
        OP_NEW_TUPLE => {
            if let Some(arena) = arena_mut(ctx.arena) {
                ctx.push(JitValue::from_ref(arena.alloc(Object::Tuple(Vec::new()))));
            } else {
                ctx.push(JitValue::null());
            }
        }
        OP_NEW_LIST => {
            if let Some(arena) = arena_mut(ctx.arena) {
                ctx.push(JitValue::from_ref(arena.alloc(Object::List(std::collections::LinkedList::new()))));
            } else {
                ctx.push(JitValue::null());
            }
        }
        OP_NEW_VECTOR => {
            if let Some(arena) = arena_mut(ctx.arena) {
                ctx.push(JitValue::from_ref(arena.alloc(Object::Vector(Vec::new()))));
            } else {
                ctx.push(JitValue::null());
            }
        }
        OP_NEW_SET => {
            if let Some(arena) = arena_mut(ctx.arena) {
                ctx.push(JitValue::from_ref(arena.alloc(Object::Set(Vec::new()))));
            } else {
                ctx.push(JitValue::null());
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_MAKE_RANGE (24)
        // ════════════════════════════════════════════════════════════════════
        OP_MAKE_RANGE => {
            let end = ctx.pop().unwrap_or(JitValue::null());
            let start = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match (start.to_int(), end.to_int()) {
                (Some(s), Some(e)) => {
                    let range: Vec<RuntimeValue> = (s..e).map(RuntimeValue::Int).collect();
                    let ptr = arena.alloc(Object::Array(range));
                    ctx.push(JitValue::from_ref(ptr));
                }
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_CAP_CALL (25)
        // ════════════════════════════════════════════════════════════════════
        OP_CAP_CALL => {
            let cap_idx = arg as usize;
            // argc was pushed onto the JIT stack by the compiler before calling
            let argc_val = ctx.pop().unwrap_or(JitValue::null());
            let argc = argc_val.to_int().unwrap_or(0) as usize;

            // Pop args from stack (top = last pushed = last arg, reverse them)
            let mut args_rtv = Vec::with_capacity(argc);
            for _ in 0..argc {
                if let Some(v) = ctx.pop() {
                    args_rtv.push(jit_to_rtv(v));
                }
            }
            args_rtv.reverse();

            // Get arena
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };

            // Look up capability from context
            if ctx.capabilities.is_null() {
                ctx.push(JitValue::null());
                return;
            }
            let caps: &Vec<std::sync::Arc<dyn crush_vm::fastvm::Capability>> =
                unsafe { &*(ctx.capabilities as *const Vec<std::sync::Arc<dyn crush_vm::fastvm::Capability>>) };

            if let Some(cap) = caps.get(cap_idx) {
                // Get hal (may be null — use a no-op hal)
                let hal: std::sync::Arc<dyn crush_vm::fastvm::Hal> = if ctx.hal.is_null() {
                    std::sync::Arc::new(DummyHal)
                } else {
                    unsafe { (&*(ctx.hal as *const std::sync::Arc<dyn crush_vm::fastvm::Hal>)).clone() }
                };

                match cap.call(arena, args_rtv, hal) {
                    Ok(result) => {
                        ctx.push(rtv_to_jit(&result));
                    }
                    Err(e) => {
                        // On error, push error string as Ref
                        let err_str = arena.alloc(Object::Str(e.to_string()));
                        ctx.push(JitValue::from_ref(err_str));
                    }
                }
            } else {
                ctx.push(JitValue::null());
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_TUPLE_PUSH (26)
        // ════════════════════════════════════════════════════════════════════
        OP_TUPLE_PUSH => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = container.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Tuple(arr)) = arena.get_mut(ref_idx) {
                        arr.push(jit_to_rtv(val));
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_LIST_PUSH (27)
        // ════════════════════════════════════════════════════════════════════
        OP_LIST_PUSH => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = container.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::List(arr)) = arena.get_mut(ref_idx) {
                        arr.push_back(jit_to_rtv(val));
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_VECTOR_PUSH (28)
        // ════════════════════════════════════════════════════════════════════
        OP_VECTOR_PUSH => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = container.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Vector(arr)) = arena.get_mut(ref_idx) {
                        arr.push(jit_to_rtv(val));
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_SET_PUSH (29)
        // ════════════════════════════════════════════════════════════════════
        OP_SET_PUSH => {
            let val = ctx.pop().unwrap_or(JitValue::null());
            let container = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = container.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Set(arr)) = arena.get_mut(ref_idx) {
                        arr.push(jit_to_rtv(val));
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_GET_FIELD (30): pop target, get field by name from strings[arg]
        // ════════════════════════════════════════════════════════════════════
        OP_GET_FIELD => {
            let field_name_idx = arg as usize;
            let target = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            match target.to_ref() {
                Some(ref_idx) => match arena.get(ref_idx) {
                    Some(Object::Object { fields, .. }) => {
                        let name = strings
                            .and_then(|ss| ss.get(field_name_idx))
                            .cloned()
                            .unwrap_or_default();
                        let val = fields.get(&name).map(rtv_to_jit).unwrap_or(JitValue::null());
                        ctx.push(val);
                    }
                    _ => ctx.push(JitValue::null()),
                },
                None => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_SET_FIELD (31): pop val, pop target, set field by name from strings[arg]
        // ════════════════════════════════════════════════════════════════════
        OP_SET_FIELD => {
            let field_name_idx = arg as usize;
            let val = ctx.pop().unwrap_or(JitValue::null());
            let target = ctx.pop().unwrap_or(JitValue::null());
            if let Some(ref_idx) = target.to_ref() {
                if let Some(arena) = arena_mut(ctx.arena) {
                    if let Ok(Object::Object { fields, .. }) = arena.get_mut(ref_idx) {
                        let name = strings
                            .and_then(|ss| ss.get(field_name_idx))
                            .cloned()
                            .unwrap_or_default();
                        fields.insert(name, jit_to_rtv(val));
                    }
                }
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_NEW_OBJ (32): create empty Object
        // ════════════════════════════════════════════════════════════════════
        OP_NEW_OBJ => {
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let ptr = arena.alloc(Object::Object {
                lang: "crush".to_string(),
                class_name: "Object".to_string(),
                fields: std::collections::HashMap::new(),
            });
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_NEW_STRUCT (33): create Object with class_name from strings[arg]
        // ════════════════════════════════════════════════════════════════════
        OP_NEW_STRUCT => {
            let name_idx = arg as usize;
            let arena = match arena_mut(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let class_name = strings
                .and_then(|ss| ss.get(name_idx))
                .cloned()
                .unwrap_or_else(|| "Object".to_string());
            let ptr = arena.alloc(Object::Object {
                lang: "crush".to_string(),
                class_name,
                fields: std::collections::HashMap::new(),
            });
            ctx.push(JitValue::from_ref(ptr));
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_STR_SIM (34): pop two strings, calculate similarity, push float
        // ════════════════════════════════════════════════════════════════════
        OP_STR_SIM => {
            let s2_val = ctx.pop().unwrap_or(JitValue::null());
            let s1_val = ctx.pop().unwrap_or(JitValue::null());
            let arena = match arena_ref(ctx.arena) {
                Some(a) => a,
                None => { ctx.push(JitValue::null()); return; }
            };
            let s1 = jit_val_to_string(s1_val, arena);
            let s2 = jit_val_to_string(s2_val, arena);
            match (s1, s2) {
                (Some(ref a), Some(ref b)) => {
                    let sim = calculate_similarity(a, b);
                    ctx.push(JitValue::float(sim));
                }
                _ => ctx.push(JitValue::null()),
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_ENTER_TRY (35): push (handler_pc, call_stack_top) onto handler stack
        // ════════════════════════════════════════════════════════════════════
        OP_ENTER_TRY => {
            let handler_pc = arg;
            if ctx.handler_stack_top < 16 {
                ctx.handler_stack[ctx.handler_stack_top] = JitHandlerFrame {
                    handler_pc,
                    call_stack_top: ctx.call_stack_top as i64,
                };
                ctx.handler_stack_top += 1;
            }
            // If handler stack is full, silently ignore (matching FastVM's no-op on overflow)
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_EXIT_TRY (36): pop from handler stack
        // ════════════════════════════════════════════════════════════════════
        OP_EXIT_TRY => {
            if ctx.handler_stack_top > 0 {
                ctx.handler_stack_top -= 1;
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // OP_THROW (37): find handler, unwind call stack, store error + handler_pc
        // ════════════════════════════════════════════════════════════════════
        OP_THROW => {
            let err_val = ctx.pop().unwrap_or(JitValue::null());

            // Walk handler stack top-to-bottom to find the nearest handler
            let mut found = false;
            let mut handler_pc: i64 = 0;

            // Walk backwards through handler stack
            let mut i = ctx.handler_stack_top;
            while i > 0 {
                i -= 1;
                let frame = ctx.handler_stack[i];

                // Check if this handler is in a currently-active call frame
                // (handler's call_stack_top <= current call_stack_top)
                if (frame.call_stack_top as usize) <= ctx.call_stack_top {
                    found = true;
                    handler_pc = frame.handler_pc;

                    // Unwind call stack to handler's level
                    ctx.call_stack_top = frame.call_stack_top as usize;

                    // Pop this handler and all above it
                    ctx.handler_stack_top = i;
                    break;
                }
            }

            if found {
                // Push error value back onto the JIT stack for the handler
                ctx.push(err_val);
                // Store handler PC and set error flag to 2 (HANDLER_FOUND).
                // CONTRACT: handler_pc is an instruction index (set by
                // OP_ENTER_TRY from the CASM EnterTry's instr.arg). The CLIF
                // emit_handler_dispatch compares against the same unit.
                // debug_assert: PC must be non-negative (i64 → usize safe).
                debug_assert!(handler_pc >= 0, "handler_pc must be non-negative (instruction index)");
                ctx.handler_pc = handler_pc as usize;
                ctx.error = 2;
            } else {
                // No handler found — set error flag to 3 (UNCAUGHT)
                ctx.error = 3;
            }
        }

        // ════════════════════════════════════════════════════════════════════
        // Unknown opcode
        // ════════════════════════════════════════════════════════════════════
        _ => {
            ctx.push(JitValue::null());
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// JitHandlerFrame
// ═══════════════════════════════════════════════════════════════════════════════

/// A single entry on the JIT exception handler stack.
///
/// Pushed by `EnterTry` and popped by `ExitTry`. When `Throw` executes, it
/// walks this stack to find the nearest enclosing handler, unwinds the
/// call stack to that frame's level, and resumes at `handler_pc`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct JitHandlerFrame {
    /// PC of the handler to jump to when an exception is thrown.
    pub handler_pc: i64,
    /// Snapshot of `call_stack_top` at the time of `EnterTry` — Throw
    /// will unwind the call stack to this level.
    pub call_stack_top: i64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// JitCallFrame
// ═══════════════════════════════════════════════════════════════════════════════

/// A single frame on the JIT call stack.
///
/// Stores the return block index (used by `brif` dispatch cascade to
/// jump back to the caller's continuation after a function returns).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct JitCallFrame {
    /// Index into the return-target table for the caller's continuation.
    pub return_block: i64,
    /// Reserved for future use (locals save, etc.).
    _reserved: i64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// JitContext
// ═══════════════════════════════════════════════════════════════════════════════

/// ABI-stable context for JIT-compiled functions.
///
/// DO NOT reorder fields without updating [`crate::compiler`]'s offset constants.
#[repr(C)]
pub struct JitContext {
    /// Value stack (inline array for fast JIT access).
    pub stack: [JitValue; JIT_STACK_SIZE],
    /// Current stack pointer (next free slot index).
    pub stack_top: usize,
    /// Local variables.
    pub locals: [JitValue; JIT_MAX_LOCALS],
    /// Number of active locals.
    pub n_locals: usize,
    /// Result value (set on Halt).
    pub result: JitValue,
    /// Budget remaining.
    pub budget: u64,
    /// Error flag (non-zero = error).
    pub error: i32,
    /// Alignment padding.
    _pad: i32,
    /// Heap arena (opaque pointer to `crush_vm::memory::Arena`).
    pub arena: *mut c_void,
    /// Capability table (opaque pointer).
    pub capabilities: *mut c_void,
    /// Host HAL (opaque pointer).
    pub hal: *mut c_void,
    /// Call stack for tracking return addresses.
    pub call_stack: [JitCallFrame; JIT_MAX_CALL_DEPTH],
    /// Current call stack pointer.
    pub call_stack_top: usize,
    /// Runtime helper function pointer (called by JIT-compiled code via call_indirect).
    pub helper_fn: *mut c_void,
    /// Non-null pointer to the program's `Vec<String>` symbol table (raw, used by helpers).
    pub strings_ptr: *const c_void,
    /// Resolved handler PC for Throw block dispatch (set by OP_THROW helper).
    ///
    /// CONTRACT (CRUSH-17 #3): The unit is an **instruction index** into the
    /// `LoweredProgram.instructions` array — the same unit used by `EnterTry`'s
    /// `instr.arg` and by `compiler.rs`'s `handler_entries` map. The CLIF
    /// `emit_handler_dispatch` brif cascade compares this value against
    /// `iconst(b, *pc as i64)` where `pc` is the EnterTry `instr.arg`.
    /// Changing the unit on either side (e.g. to a byte offset) without updating
    /// the other will silently make ALL throw dispatches uncatchable.
    pub handler_pc: usize,
    /// Exception handler stack pointer.
    pub handler_stack_top: usize,
    /// Exception handler stack (max 16 nested try blocks).
    pub handler_stack: [JitHandlerFrame; 16],
}

// ── Ensure layout is as expected ────────────────────────────────────────────

const _: () = {
    assert!(core::mem::size_of::<JitContext>() >= 10088);
    assert!(core::mem::size_of::<JitContext>() <= 10112);
    assert!(core::mem::align_of::<JitContext>() == 8);
    assert!(core::mem::offset_of!(JitContext, stack) == 0);
    assert!(core::mem::offset_of!(JitContext, stack_top) == 8192);
    assert!(core::mem::offset_of!(JitContext, locals) == 8200);
    assert!(core::mem::offset_of!(JitContext, result) == 8720);
    assert!(core::mem::offset_of!(JitContext, budget) == 8728);
    assert!(core::mem::offset_of!(JitContext, error) == 8736);
    assert!(core::mem::offset_of!(JitContext, call_stack) == 8768);
    assert!(core::mem::offset_of!(JitContext, call_stack_top) == 9792);
    assert!(core::mem::offset_of!(JitContext, helper_fn) == 9800);
    assert!(core::mem::offset_of!(JitContext, strings_ptr) == 9808);
    assert!(core::mem::offset_of!(JitContext, handler_pc) == 9816);
    assert!(core::mem::offset_of!(JitContext, handler_stack_top) == 9824);
    assert!(core::mem::offset_of!(JitContext, handler_stack) == 9832);
};

impl JitContext {
    /// Create a new context with all fields zeroed/null.
    pub fn new() -> Self {
        Self {
            stack: [JitValue(TAG_NULL); JIT_STACK_SIZE],
            stack_top: 0,
            locals: [JitValue(TAG_NULL); JIT_MAX_LOCALS],
            n_locals: 0,
            result: JitValue(TAG_NULL),
            budget: 1_000_000,
            error: 0,
            _pad: 0,
            arena: std::ptr::null_mut(),
            capabilities: std::ptr::null_mut(),
            hal: std::ptr::null_mut(),
            call_stack: [JitCallFrame { return_block: 0, _reserved: 0 }; JIT_MAX_CALL_DEPTH],
            call_stack_top: 0,
            helper_fn: jit_helper_noop as *mut c_void,
            strings_ptr: std::ptr::null(),
            handler_pc: 0,
            handler_stack_top: 0,
            handler_stack: [JitHandlerFrame { handler_pc: 0, call_stack_top: 0 }; 16],
        }
    }

    /// Push a value onto the shadow stack.
    #[inline]
    pub fn push(&mut self, val: JitValue) {
        debug_assert!(self.stack_top < JIT_STACK_SIZE, "JIT stack overflow");
        self.stack[self.stack_top] = val;
        self.stack_top += 1;
    }

    /// Pop a value from the shadow stack.
    #[inline]
    pub fn pop(&mut self) -> Option<JitValue> {
        if self.stack_top == 0 {
            None
        } else {
            self.stack_top -= 1;
            Some(self.stack[self.stack_top])
        }
    }

    /// Peek at the top of the stack.
    #[inline]
    pub fn top(&self) -> Option<JitValue> {
        if self.stack_top == 0 {
            None
        } else {
            Some(self.stack[self.stack_top - 1])
        }
    }

    /// Read the result value.
    #[inline]
    pub fn result(&self) -> JitValue {
        self.result
    }

    /// Store the result value.
    #[inline]
    pub fn set_result(&mut self, val: JitValue) {
        self.result = val;
    }
}

impl Default for JitContext {
    fn default() -> Self {
        Self::new()
    }
}
