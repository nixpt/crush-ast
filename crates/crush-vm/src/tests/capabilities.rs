//! Tests for the capability dispatch domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1.
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── capabilities ─────────────────────────────────────────────────────────────

#[test]
fn cap_io_print() {
    let r = run_src_with_perms(
        "PUSH_STR \"hello\"\nCAP_CALL \"io.print\" 1\nHALT",
        &["io.print"],
    );
    assert_eq!(r.output, "hello");
}

#[test]
fn cap_str_concat() {
    let r = run_src_with_perms(
        "PUSH_STR \"foo\"\nPUSH_STR \"bar\"\nCAP_CALL \"str.concat\" 2\nHALT",
        &["str.concat"],
    );
    assert_eq!(r.stack, vec![Value::Str("foobar".to_string())]);
}

#[test]
fn cap_str_len() {
    let r = run_src_with_perms(
        "PUSH_STR \"hello\"\nCAP_CALL \"str.len\" 1\nHALT",
        &["str.len"],
    );
    assert_eq!(r.stack, vec![Value::Int(5)]);
}

#[test]
fn cap_not_declared_errors() {
    let prog = assemble("PUSH_STR \"hi\"\nCAP_CALL \"io.print\" 1\nHALT", None, None).unwrap();
    assert!(run(&prog, &Quotas::default()).is_err());
}

// ── capability tests ──────────────────────────────────────────────────────────

#[test]
fn cap_str_contains() {
    let r = run_src_with_perms(
        r#"PUSH_STR "hello world"
    PUSH_STR "world"
    CAP_CALL "str.contains" 2
    HALT"#,
        &["str.contains"],
    );
    assert_eq!(r.stack, vec![Value::Bool(true)]);
}

#[test]
fn cap_str_split() {
    let r = run_src_with_perms(
        r#"PUSH_STR "a,b,c"
    PUSH_STR ","
    CAP_CALL "str.split" 2
    HALT"#,
        &["str.split"],
    );
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.borrow().len(), 3);
    } else {
        panic!("expected array");
    }
}

#[test]
fn cap_str_replace() {
    let r = run_src_with_perms(
        r#"PUSH_STR "hello world"
    PUSH_STR "world"
    PUSH_STR "there"
    CAP_CALL "str.replace" 3
    HALT"#,
        &["str.replace"],
    );
    assert_eq!(r.stack, vec![Value::Str("hello there".to_string())]);
}

#[test]
fn cap_str_join() {
    let r = run_src_with_perms(
        r#"PUSH_STR "a"
    PUSH_STR "b"
    NEW_ARRAY 2
    PUSH_STR ","
    CAP_CALL "str.join" 2
    HALT"#,
        &["str.join"],
    );
    assert_eq!(r.stack, vec![Value::Str("a,b".to_string())]);
}

#[test]
fn cap_make_range() {
    let r = run_src_with_perms(
        "PUSH 0\nPUSH 5\nCAP_CALL \"make_range\" 2\nHALT",
        &["make_range"],
    );
    if let Some(Value::Array(arr)) = r.stack.first() {
        assert_eq!(arr.borrow().len(), 5);
    } else {
        panic!("expected array");
    }
}

fn map_is_truthy_only_when_non_empty() {
    assert!(!Value::new_map(std::collections::HashMap::new()).is_truthy());
    let mut m = std::collections::HashMap::new();
    m.insert("x".to_string(), Value::Int(1));
    assert!(Value::new_map(m).is_truthy());
}
