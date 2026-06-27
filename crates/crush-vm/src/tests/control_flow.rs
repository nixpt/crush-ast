//! Tests for the control flow, slots (memory), function calls domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── control flow ──────────────────────────────────────────────────────────────

#[test]
fn unconditional_jump() {
    // Jump over a PUSH that would put 99 on the stack.
    let r = run_src("JMP done\nPUSH 99\ndone:\nPUSH 1\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn jz_taken() {
    let r = run_src("PUSH 0\nJZ end\nPUSH 99\nend:\nPUSH 1\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1)]);
}

#[test]
fn jz_not_taken() {
    let r = run_src("PUSH 5\nJZ skip\nPUSH 42\nskip:\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
}

#[test]
fn countdown_loop() {
    // count down from 3 to 0 using JNZ.
    let src = "PUSH 3\nloop:\nPUSH 1\nSUB\nDUP\nJNZ loop\nHALT";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(0)]);
}

// ── slots (memory) ───────────────────────────────────────────────────────────

#[test]
fn store_and_load() {
    let r = run_src("PUSH 42\nSTORE 0\nLOAD 0\nHALT");
    assert_eq!(r.stack, vec![Value::Int(42)]);
}

// ── functions (v2) ───────────────────────────────────────────────────────────

#[test]
fn function_call_and_ret() {
    let src = "\
.func main
    PUSH 1
    CALL double
    HALT
.func double
    PUSH 2
    MUL
    RET
";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(2)]);
}
