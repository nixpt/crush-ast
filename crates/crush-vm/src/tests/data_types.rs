//! Tests for the strings, arrays, new types, native string ops domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── strings ──────────────────────────────────────────────────────────────────

#[test]
fn push_str() {
    let r = run_src("PUSH_STR \"hello\"\nHALT");
    assert_eq!(r.stack, vec![Value::Str("hello".to_string())]);
}

#[test]
fn string_ops() {
    let src = "
    PUSH_STR \"hello world\"
    PUSH_STR \"world\"
    STR_CONTAINS
    PUSH_STR \"hello\"
    PUSH_STR \"hell\"
    STR_STARTS_WITH
    PUSH_STR \"world\"
    PUSH_STR \"ld\"
    STR_ENDS_WITH
    PUSH_STR \" UPPER \"
    STR_TRIM
    STR_TO_LOWER
    PUSH_STR \"lower\"
    STR_TO_UPPER
    HALT
    ";
    let r = run_src(src);
    assert_eq!(
        r.stack,
        vec![
            Value::Bool(true),
            Value::Bool(true),
            Value::Bool(true),
            Value::Str("upper".to_string()),
            Value::Str("LOWER".to_string()),
        ]
    );
}

#[test]
fn push_null() {
    let r = run_src("PUSH_NULL\nHALT");
    assert_eq!(r.stack, vec![Value::Null]);
}

// ── arrays ────────────────────────────────────────────────────────────────────

#[test]
fn new_array() {
    let r = run_src("PUSH 1\nPUSH 2\nPUSH 3\nNEW_ARRAY 3\nHALT");
    assert_eq!(
        r.stack,
        vec![Value::new_array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3)
        ])]
    );
}

#[test]
fn arr_get_and_len() {
    let r = run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nARR_LEN\nHALT");
    assert_eq!(r.stack, vec![Value::Int(2)]);
    let r = run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nPUSH 0\nARR_GET\nHALT");
    assert_eq!(r.stack, vec![Value::Int(10)]);
}

#[test]
fn arr_set() {
    let r =
        run_src("PUSH 10\nPUSH 20\nNEW_ARRAY 2\nPUSH 0\nPUSH 99\nARR_SET\nPUSH 0\nARR_GET\nHALT");
    assert_eq!(r.stack, vec![Value::Int(99)]);
}

// ── new types: Bool, Map, Error, Bytes, ARR_PUSH/POP ─────────────────────────

#[test]
fn push_bool_works() {
    let r = run_src("PUSH_BOOL 1\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH_BOOL 0\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn new_obj_creates_empty_map() {
    let r = run_src("NEW_OBJ\nHALT");
    assert_eq!(r.stack.len(), 1);
    assert!(matches!(r.stack[0], Value::Map(_)));
}

#[test]
fn set_field_and_get_field() {
    // NEW_OBJ, DUP, PUSH_STR "hello", SET_FIELD "greeting", GET_FIELD "greeting"
    let r = run_src(
        r#"NEW_OBJ
    DUP
    PUSH_STR "hello"
    SET_FIELD "greeting"
    GET_FIELD "greeting"
    HALT"#,
    );
    assert_eq!(r.stack.len(), 2);
    assert!(matches!(r.stack[0], Value::Map(_)));
    assert_eq!(r.stack[1], Value::Str("hello".to_string()));
}

#[test]
fn get_field_missing_returns_null() {
    let r = run_src("NEW_OBJ\nGET_FIELD \"missing\"\nHALT");
    assert_eq!(r.stack, vec![Value::Null]);
}

#[test]
fn map_type_name() {
    let r = run_src("NEW_OBJ\nHALT");
    assert_eq!(r.stack[0].type_name(), "map");
}

#[test]
fn throw_basic() {
    let prog = assemble("PUSH_STR \"oops\"\nTHROW\nHALT", None, None).unwrap();
    let result = run(&prog, &Quotas::default());
    assert!(result.is_err());
}

#[test]
fn enter_try_and_exit_try_no_error() {
    // try { push 1 } catch { push 2 }
    let r = run_src("ENTER_TRY handler\nPUSH 1\nEXIT_TRY\nJMP done\nhandler:\nPUSH 2\ndone:\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn try_catch_catches_throw() {
    // try { throw "err" } catch { pop error, push 99 }
    // THROW pushes the error value onto the stack before jumping to handler
    let r = run_src(
        "ENTER_TRY handler\nPUSH_STR \"err\"\nTHROW\nEXIT_TRY\nJMP done\nhandler:\nPOP\nPUSH 99\ndone:\nHALT",
    );
    assert_eq!(r.stack, vec![Value::Int(99)]);
}

#[test]
fn throw_error_value_on_stack_in_handler() {
    // try { throw "msg" } catch { the error value is already on stack }
    let r = run_src(
        "ENTER_TRY handler\nPUSH_STR \"msg\"\nTHROW\nEXIT_TRY\nJMP done\nhandler:\nHALT\ndone:\nHALT",
    );
    // After THROW, the error "msg" is pushed onto the stack for handler
    assert_eq!(r.stack, vec![Value::Str("msg".to_string())]);
}

#[test]
fn bool_type_name() {
    assert_eq!(Value::Bool(true).type_name(), "bool");
    assert_eq!(Value::Bool(false).type_name(), "bool");
}

#[test]
fn bool_as_text() {
    assert_eq!(Value::Bool(true).as_text(), "true");
    assert_eq!(Value::Bool(false).as_text(), "false");
}

#[test]
fn error_type_name_and_text() {
    let e = Value::Error("test error".to_string());
    assert_eq!(e.type_name(), "error");
    assert!(e.as_text().contains("test error"));
    assert!(e.is_truthy());
}

#[test]
fn bytes_type_name_and_text() {
    let b = Value::Bytes(vec![1, 2, 3]);
    assert_eq!(b.type_name(), "bytes");
    assert!(b.as_text().contains("3 bytes"));
    assert!(b.is_truthy());
    assert!(!Value::Bytes(vec![]).is_truthy());
}

#[test]
fn bool_is_not_numeric() {
    assert!(!Value::Bool(true).is_numeric());
    assert!(!Value::Bool(false).is_numeric());
}

#[test]
fn arr_push_and_arr_pop() {
    // Build [1, 2] without using DUP (so stack is clean):
    // NEW_ARRAY 0, PUSH 1, ARR_PUSH, PUSH 2, ARR_PUSH
    // But ARR_PUSH needs the array on the stack too.
    // With NEW_ARRAY → DUP → PUSH → ARR_PUSH, the DUP leaves a copy.
    // After the sequence, the last stack item is the final array.
    let r = run_src("NEW_ARRAY 0\nDUP\nPUSH 1\nARR_PUSH\nDUP\nPUSH 2\nARR_PUSH\nHALT");
    let last = r.stack.last().expect("should have a value");
    if let Value::Array(arr) = last {
        assert_eq!(arr.borrow().len(), 2);
        assert_eq!(arr.borrow()[0], Value::Int(1));
        assert_eq!(arr.borrow()[1], Value::Int(2));
    } else {
        panic!("expected array, got {:?}", last);
    }
}

#[test]
fn arr_pop_removes_last() {
    let r = run_src(
        "NEW_ARRAY 0\nDUP\nPUSH 1\nARR_PUSH\nDUP\nPUSH 2\nARR_PUSH\nDUP\nPUSH 3\nARR_PUSH\nARR_POP\nPOP\nARR_POP\nHALT",
    );
    // Stack after NEW_ARRAY + 3 pushes + 2 pops + 1 pop:
    // After all ARR_PUSH ops: stack has old copies + final array
    // Last element should be 2 (popped value from second ARR_POP)
    // Second-to-last should be [1] (array after popping 2)
    let len = r.stack.len();
    assert!(len >= 2, "expected at least 2 values, got {}", len);
    if let Value::Int(v) = &r.stack[len - 1] {
        assert_eq!(*v, 2);
    } else {
        panic!("expected Int, got {:?}", r.stack[len - 1]);
    }
    if let Value::Array(arr) = &r.stack[len - 2] {
        assert_eq!(arr.borrow().len(), 1);
        assert_eq!(arr.borrow()[0], Value::Int(1));
    } else {
        panic!("expected Array, got {:?}", r.stack[len - 2]);
    }
}

// ── native string ops ──────────────────────────────────────────────────────────

#[test]
fn str_contains_native() {
    let r = run_src(r#"PUSH_STR "hello world"
    PUSH_STR "world"
    STR_CONTAINS
    HALT"#);
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src(r#"PUSH_STR "hello"
    PUSH_STR "xyz"
    STR_CONTAINS
    HALT"#);
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn str_split_native() {
    let r = run_src(r#"PUSH_STR "a,b,c"
    PUSH_STR ","
    STR_SPLIT
    HALT"#);
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.borrow().len(), 3);
        assert_eq!(arr.borrow()[0], Value::Str("a".to_string()));
        assert_eq!(arr.borrow()[1], Value::Str("b".to_string()));
        assert_eq!(arr.borrow()[2], Value::Str("c".to_string()));
    } else {
        panic!("expected array");
    }
}

#[test]
fn str_replace_native() {
    let r = run_src(r#"PUSH_STR "hello world"
    PUSH_STR "world"
    PUSH_STR "there"
    STR_REPLACE
    HALT"#);
    assert_eq!(r.stack, vec![Value::Str("hello there".to_string())]);
}

#[test]
fn str_join_native() {
    let r = run_src(r#"PUSH_STR "a"
    PUSH_STR "b"
    PUSH_STR "c"
    NEW_ARRAY 3
    PUSH_STR ","
    STR_JOIN
    HALT"#);
    assert_eq!(r.stack, vec![Value::Str("a,b,c".to_string())]);
}

#[test]
fn make_range_native() {
    let r = run_src("PUSH 0\nPUSH 5\nMAKE_RANGE\nHALT");
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.borrow().len(), 5);
        assert_eq!(arr.borrow()[0], Value::Int(0));
        assert_eq!(arr.borrow()[4], Value::Int(4));
    } else {
        panic!("expected array");
    }
    let r = run_src("PUSH 5\nPUSH 3\nMAKE_RANGE\nHALT");  // empty range
    assert_eq!(r.stack, vec![Value::new_array(vec![])]);
}
