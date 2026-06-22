//! Tests for the async green threads (spawn / await / yield) domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── async / green threads ─────────────────────────────────────────────────────

#[test]
fn spawn_creates_handle() {
    // SPAWN with a function name should push a Handle, not null
    let src = "\
.func main
    PUSH_STR \"other\"
    SPAWN
    HALT
.func other
    HALT";
    let r = run_src(src);
    let top = r.stack.last().expect("should have a value");
    assert!(matches!(top, Value::Handle(_)), "expected Handle, got {top:?}");
}

#[test]
fn spawn_await_roundtrip() {
    // SPAWN a function, AWAIT it, check result
    let src = "\
.func main
    PUSH_STR \"worker\"
    SPAWN
    AWAIT
    HALT
.func worker
    PUSH 42
    HALT";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(42)]);
}

#[test]
fn yield_does_not_crash() {
    let r = run_src("PUSH 1\nYIELD\nPUSH 2\nHALT");
    assert_eq!(r.stack, vec![Value::Int(1), Value::Int(2)]);
}
