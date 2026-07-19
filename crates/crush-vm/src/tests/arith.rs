//! Tests for the arithmetic, stack ops, bitwise ops domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── arithmetic ────────────────────────────────────────────────────────────────

#[test]
fn push_and_halt() {
    let r = run_src("PUSH 42\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
    assert!(r.halted);
}

#[test]
fn add_integers() {
    let r = run_src("PUSH 3\nPUSH 4\nADD\nHALT");
    assert_eq!(r.stack, vec![Value::Int(7)]);
}

#[test]
fn sub_mul_div() {
    let r = run_src("PUSH 10\nPUSH 3\nSUB\nHALT");
    assert_eq!(r.stack, vec![Value::Int(7)]);
    let r = run_src("PUSH 6\nPUSH 7\nMUL\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
    let r = run_src("PUSH 10\nPUSH 3\nDIV\nHALT");
    assert_eq!(r.stack, vec![Value::Int(3)]);
}

#[test]
fn math_funcs() {
    let src = "
    PUSH 2
    PUSH 3
    MATH_POW
    PUSH_F64 16.0
    MATH_SQRT
    PUSH -5
    MATH_ABS
    PUSH_F64 2.5
    MATH_ROUND
    PUSH_F64 2.7
    MATH_FLOOR
    PUSH_F64 2.1
    MATH_CEIL
    HALT
    ";
    let r = run_src(src);
    assert_eq!(
        r.stack,
        vec![
            Value::Float(8.0),
            Value::Float(4.0),
            Value::Float(5.0),
            Value::Float(3.0),
            Value::Float(2.0),
            Value::Float(3.0),
        ]
    );
}

#[test]
fn modulo() {
    let r = run_src("PUSH 10\nPUSH 3\nMOD\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn mod_negative_values() {
    let r = run_src("PUSH -7\nPUSH 3\nMOD\nHALT");
    assert_eq!(r.stack, vec![Value::Int(-1)]);  // Rust-style truncation: -7 - 3*(-2) = -1
}

#[test]
fn float_push() {
    let r = run_src("PUSH_F64 3.14\nHALT");
    assert!(matches!(r.stack.first(), Some(Value::Float(f)) if (f - 3.14).abs() < 1e-10));
}

#[test]
fn comparisons() {
    let r = run_src("PUSH 3\nPUSH 5\nLT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 3\nGT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
}

#[test]
fn logical_not() {
    let r = run_src("PUSH 0\nNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 42\nNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn negate() {
    let r = run_src("PUSH 42\nNEG\nHALT");
    assert_eq!(r.stack, vec![Value::Int(-42)]);
    let r = run_src("PUSH_F64 3.5\nNEG\nHALT");
    assert!(matches!(r.stack.first(), Some(Value::Float(f)) if (*f - (-3.5)).abs() < 1e-10));
}

#[test]
fn extended_comparisons() {
    let r = run_src("PUSH 3\nPUSH 5\nNE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nNE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 3\nPUSH 5\nLE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nLE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 6\nPUSH 5\nLE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 6\nPUSH 5\nGE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 5\nPUSH 5\nGE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 3\nPUSH 5\nGE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn eq_ne_cross_type_int_float() {
    // CRUSHVM-EQ-1: `2 == 2.0` is `true`, matching chroma's Python VM
    // (Python's `2 == 2.0` is `True`). Canonical fix lives in
    // `Value`'s `PartialEq` impl (crates/crush-vm/src/vm.rs).
    let r = run_src("PUSH 2\nPUSH_F64 2.0\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH_F64 2.0\nPUSH 2\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);

    // NE flows through the same comparison (NE = !EQ) -- 2 != 2.1 is true.
    let r = run_src("PUSH 2\nPUSH_F64 2.1\nNE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    // ... and 2 != 2.0 is false, mirroring the EQ case above.
    let r = run_src("PUSH 2\nPUSH_F64 2.0\nNE\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);

    // Negative case: cross-type but NOT numerically equal.
    let r = run_src("PUSH 2\nPUSH_F64 3.0\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);

    // Same-type float/float and int/int EQ are unaffected (sanity).
    let r = run_src("PUSH_F64 2.0\nPUSH_F64 2.0\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 2\nPUSH 2\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
}

#[test]
fn eq_nan_never_equal() {
    // NaN must never compare equal to anything, including itself --
    // the numeric int/float EQ widening must not disturb this.
    let r = run_src("PUSH_F64 NaN\nPUSH_F64 NaN\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 2\nPUSH_F64 NaN\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn eq_bool_int_not_coerced() {
    // Only (Int, Float) / (Float, Int) are coerced -- Bool vs Int stays
    // unequal even when the underlying "truthiness" would line up.
    let r = run_src("PUSH_BOOL 1\nPUSH 1\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH_BOOL 0\nPUSH 0\nEQ\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

#[test]
fn logical_and_or() {
    let r = run_src("PUSH 1\nPUSH 42\nAND\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 0\nPUSH 42\nAND\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
    let r = run_src("PUSH 0\nPUSH 42\nOR\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(true)]);
    let r = run_src("PUSH 0\nPUSH 0\nOR\nHALT");
    assert_eq!(r.stack, vec![Value::Bool(false)]);
}

// ── stack ops ────────────────────────────────────────────────────────────────

#[test]
fn dup_and_swap() {
    let r = run_src("PUSH 1\nPUSH 2\nSWAP\nHALT");
    assert_eq!(r.stack, vec![Value::Int(2), Value::Int(1)]);
    let r = run_src("PUSH 7\nDUP\nHALT");
    assert_eq!(r.stack, vec![Value::Int(7), Value::Int(7)]);
}

#[test]
fn pop_removes_top() {
    let r = run_src("PUSH 1\nPUSH 2\nPOP\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

// ── bitwise ops ───────────────────────────────────────────────────────────────

#[test]
fn bitwise_and_or_xor() {
    let r = run_src("PUSH 12\nPUSH 10\nBITAND\nHALT");  // 12&10 = 8
    assert_eq!(r.stack, vec![Value::Int(8)]);
    let r = run_src("PUSH 12\nPUSH 10\nBITOR\nHALT");   // 12|10 = 14
    assert_eq!(r.stack, vec![Value::Int(14)]);
    let r = run_src("PUSH 12\nPUSH 10\nBITXOR\nHALT");  // 12^10 = 6
    assert_eq!(r.stack, vec![Value::Int(6)]);
}

#[test]
fn bitwise_not_shift() {
    let r = run_src("PUSH 0\nBITNOT\nHALT");
    assert_eq!(r.stack, vec![Value::Int(-1)]);
    let r = run_src("PUSH 1\nPUSH 4\nSHL\nHALT");  // 1<<4 = 16
    assert_eq!(r.stack, vec![Value::Int(16)]);
    let r = run_src("PUSH 16\nPUSH 2\nSHR\nHALT");  // 16>>2 = 4
    assert_eq!(r.stack, vec![Value::Int(4)]);
}
