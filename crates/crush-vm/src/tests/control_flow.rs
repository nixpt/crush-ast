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

// ── VM faults are catchable (CRUSH-369) ──────────────────────────────────────

use crate::block_capture::OutputCapture;
use crate::vm::{Frame, TryHandler, VmError, raise_fault_to_handler};

/// `run_src` panics on Err; the fatal-fault tests need the raw result.
fn run_src_raw(src: &str) -> Result<crate::vm::VmResult, VmError> {
    let prog = assemble(src, None, None).expect("assembly");
    run(&prog, &Quotas::default())
}

#[test]
fn a_fault_inside_try_is_caught_as_an_error_value() {
    // try { 1/0 } catch { } — the DivByZero fault unwinds to the handler as a
    // caught Value::Error carrying the fault's message, and execution resumes
    // at the handler, exactly like a THROW of that value would.
    let r = run_src(
        "ENTER_TRY handler\nPUSH 1\nPUSH 0\nDIV\nEXIT_TRY\nJMP done\nhandler:\nHALT\ndone:\nHALT",
    );
    assert_eq!(r.stack, vec![Value::Error("division by zero".to_string())]);
}

#[test]
fn a_fault_without_try_still_aborts_with_the_same_fault() {
    let result = run_src_raw("PUSH 1\nPUSH 0\nDIV\nHALT");
    assert!(matches!(result, Err(VmError::DivByZero)), "{result:?}");
}

#[test]
fn a_fault_in_a_callee_unwinds_to_the_callers_try() {
    // The fault is raised two frames below the try (CRUSH-126's unwind).
    let src = "\
.func main
    ENTER_TRY handler
    PUSH 0
    CALL boom
    EXIT_TRY
    JMP done
handler:
    HALT
done:
    HALT
.func boom
    PUSH 1
    PUSH 0
    DIV
    RET
";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Error("division by zero".to_string())]);
}

#[test]
fn a_rethrow_from_catch_reaches_the_outer_try() {
    // The inner try's handler rethrows (THROW of the caught value); the outer
    // try catches it. The inner handler was already popped by the first unwind.
    let src = "\
.func main
    ENTER_TRY outer
    ENTER_TRY inner
    PUSH 1
    PUSH 0
    DIV
inner:
    THROW
outer:
    HALT
";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Error("division by zero".to_string())]);
}

#[test]
fn call_depth_quota_is_not_catchable() {
    // Quotas stay fatal (CRUSH-369): infinite recursion inside try still ends
    // the run with the quota error, not a caught value.
    let src = "\
.func main
    ENTER_TRY handler
    PUSH 0
    CALL recurse
    EXIT_TRY
    JMP done
handler:
    POP
    PUSH 99
done:
    HALT
.func recurse
    PUSH 1
    CALL recurse
    RET
";
    let result = run_src_raw(src);
    assert!(
        matches!(result, Err(VmError::CallDepthQuota(_))),
        "{result:?}"
    );
}

#[test]
fn fault_unwinding_past_an_open_polyglot_capture_fails_the_block() {
    // A catchable fault with no try handler inside an inline-lowered polyglot
    // block's capture window is the block failing (#312): fatal, as a
    // LangRuntimeError, not a caught value.
    let mut captures = vec![OutputCapture {
        lang: "python".to_string(),
        line: Some(3),
        call_depth: 1,
        buf: String::new(),
    }];
    let mut try_stack = Vec::new();
    let mut call_stack = vec![
        Frame {
            return_ip: None,
            memory: std::collections::HashMap::new(),
            on_ret: crate::vm::RetValue::Keep,
        },
        Frame {
            return_ip: None,
            memory: std::collections::HashMap::new(),
            on_ret: crate::vm::RetValue::Keep,
        },
    ];
    let mut stack = Vec::new();
    let err = raise_fault_to_handler(
        VmError::DivByZero,
        &mut try_stack,
        &mut call_stack,
        &mut stack,
        &mut captures,
    )
    .expect_err("the block failed");
    assert!(matches!(err, VmError::LangRuntimeError { .. }), "{err:?}");
    assert!(captures.is_empty(), "the escaped captures are dropped");
}

#[test]
fn fault_unwinds_to_a_handler_inside_the_polyglot_block() {
    // A handler entered inside the block's capture window (deeper than the
    // capture's call_depth) catches the fault normally; the capture stays open.
    let mut captures = vec![OutputCapture {
        lang: "python".to_string(),
        line: Some(3),
        call_depth: 1,
        buf: String::new(),
    }];
    let mut try_stack = vec![TryHandler {
        handler_ip: 7,
        call_depth: 2,
        stack_len: 0,
    }];
    let mut call_stack = vec![
        Frame {
            return_ip: None,
            memory: std::collections::HashMap::new(),
            on_ret: crate::vm::RetValue::Keep,
        },
        Frame {
            return_ip: None,
            memory: std::collections::HashMap::new(),
            on_ret: crate::vm::RetValue::Keep,
        },
    ];
    let mut stack = Vec::new();
    let handler_ip = raise_fault_to_handler(
        VmError::DivByZero,
        &mut try_stack,
        &mut call_stack,
        &mut stack,
        &mut captures,
    )
    .expect("caught inside the block");
    assert_eq!(handler_ip, 7);
    assert_eq!(stack, vec![Value::Error("division by zero".to_string())]);
    assert_eq!(captures.len(), 1, "the capture stays open");
    assert!(try_stack.is_empty(), "the handler was consumed");
}

#[test]
fn uncatchable_faults_are_refused_by_the_shared_unwinder() {
    // Quota exhaustion and a deliberate sys.exit stay fatal even with a
    // handler active: the unwinder returns them untouched.
    let mut try_stack = vec![TryHandler {
        handler_ip: 7,
        call_depth: 1,
        stack_len: 0,
    }];
    for fatal in [VmError::CallDepthQuota(64), VmError::Exit(3)] {
        let mut call_stack = vec![
            Frame {
                return_ip: None,
                memory: std::collections::HashMap::new(),
                on_ret: crate::vm::RetValue::Keep,
            },
            Frame {
                return_ip: None,
                memory: std::collections::HashMap::new(),
                on_ret: crate::vm::RetValue::Keep,
            },
        ];
        let mut stack = Vec::new();
        let mut captures = Vec::new();
        let out = raise_fault_to_handler(
            fatal,
            &mut try_stack,
            &mut call_stack,
            &mut stack,
            &mut captures,
        )
        .expect_err("stays fatal");
        assert!(
            matches!(out, VmError::CallDepthQuota(_) | VmError::Exit(_)),
            "{out:?}"
        );
        assert!(stack.is_empty(), "nothing was pushed for {out:?}");
    }
}
