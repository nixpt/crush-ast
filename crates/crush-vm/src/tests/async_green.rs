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
    SPAWN 0
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
    SPAWN 0
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

#[test]
fn spawn_with_args() {
    // SPAWN a function with 1 arg, AWAIT it, check result
    // The arg is on the spawned thread's stack. PUSH 2 * POP = MUL
    let src = "\
.func main
    PUSH 99
    PUSH_STR \"double\"
    SPAWN 1
    AWAIT
    HALT
.func double
    PUSH 2
    MUL
    HALT";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(198)], "expected 99*2=198, got {:?}", r.stack);
}

#[test]
fn spawn_with_multiple_args() {
    // SPAWN a function with 2 args, AWAIT it, check result
    let src = "\
.func main
    PUSH 3
    PUSH 7
    PUSH_STR \"add\"
    SPAWN 2
    AWAIT
    HALT
.func add
    ADD
    HALT";
    let r = run_src(src);
    assert_eq!(r.stack, vec![Value::Int(10)], "expected 3+7=10, got {:?}", r.stack);
}

// ── #177 regressions: spawned-task output and exit semantics ──────────────────
//
// #177 fixed two scheduler bugs: spawned threads' output was dropped (each
// thread printed into its own buffer and only main's was returned), and its
// first version then appended a thread's output only when that thread
// finished — so interleaved prints came out of order. The fix shares one
// output buffer across threads and keeps Go-style exit: main returning ends
// the program. These pin all of that down.
//
// Functions end with RET, as compiled Crush does (the compiler never emits
// HALT at a function end). HALT in any thread ends the whole program.

/// main and a worker alternate via YIELD; prints must appear in execution order.
const INTERLEAVED: &str = "\
.func main
    PUSH_STR \"worker\"
    SPAWN 0
    POP
    YIELD
    PUSH_STR \"m1\"
    CAP_CALL \"io.print\" 1
    YIELD
    PUSH_STR \"m2\"
    CAP_CALL \"io.print\" 1
    RET
.func worker
    PUSH_STR \"w1\"
    CAP_CALL \"io.print\" 1
    YIELD
    PUSH_STR \"w2\"
    CAP_CALL \"io.print\" 1
    RET";

#[test]
fn spawned_output_keeps_execution_order() {
    let r = run_src_with_perms(INTERLEAVED, &["io.print"]);
    assert_eq!(r.output, "w1\nm1\nw2\nm2\n");
}

#[test]
fn spawned_output_streams_in_execution_order() {
    let prog = assemble(INTERLEAVED, Some(&["io.print"]), None).expect("assembly");
    let mut chunks: Vec<String> = Vec::new();
    let r = crate::vm::run_with_caps_streaming(&prog, &Quotas::default(), None, &mut |s| {
        chunks.push(s.to_string())
    })
    .expect("vm run");
    assert_eq!(chunks.concat(), "w1\nm1\nw2\nm2\n", "streamed chunks: {chunks:?}");
    assert_eq!(r.output, chunks.concat());
}

#[test]
fn main_returning_ends_program_with_a_never_ending_task() {
    // Go-style: a background loop must not keep the program alive (or turn a
    // clean exit into an instruction-quota error).
    let src = "\
.func main
    PUSH_STR \"spin\"
    SPAWN 0
    POP
    YIELD
    PUSH_STR \"main done\"
    CAP_CALL \"io.print\" 1
    RET
.func spin
loop:
    YIELD
    JMP loop";
    let prog = assemble(src, Some(&["io.print"]), None).expect("assembly");
    let quotas = Quotas { max_steps: 10_000, ..Quotas::default() };
    let r = run(&prog, &quotas).expect("main returning must end the program cleanly");
    assert_eq!(r.output, "main done\n");
}

#[test]
fn awaited_worker_output_precedes_its_result() {
    let src = "\
.func main
    PUSH_STR \"worker\"
    SPAWN 0
    AWAIT
    CAP_CALL \"io.print\" 1
    RET
.func worker
    PUSH_STR \"working\"
    CAP_CALL \"io.print\" 1
    PUSH 42
    RET";
    let r = run_src_with_perms(src, &["io.print"]);
    assert_eq!(r.output, "working\n42\n");
}

#[test]
fn worker_output_is_kept_when_it_finishes_after_main_yields() {
    // Before #177 the worker's print was lost: only main's buffer was returned.
    let src = "\
.func main
    PUSH_STR \"worker\"
    SPAWN 0
    POP
    YIELD
    PUSH_STR \"main\"
    CAP_CALL \"io.print\" 1
    RET
.func worker
    PUSH_STR \"from worker\"
    CAP_CALL \"io.print\" 1
    RET";
    let r = run_src_with_perms(src, &["io.print"]);
    assert_eq!(r.output, "from worker\nmain\n");
}

// ── #161: the portable VM (crush-web) runs green tasks like the scheduler ─────
//
// The portable VM used to record a spawn and run it only when awaited; YIELD
// was a no-op. Each program here runs on both engines and must print the
// same thing (and the expected text).

fn run_both(src: &str) -> String {
    let prog = assemble(src, Some(&["io.print"]), None).expect("assembly");
    let native = run(&prog, &Quotas::default()).expect("scheduler run").output;
    let mut pvm = crate::portable_vm::PortableVm::new(prog);
    let portable = pvm.run().expect("portable run").output;
    assert_eq!(portable, native, "portable VM and scheduler disagree");
    native
}

#[test]
fn engines_agree_yield_runs_spawned_worker() {
    // The program from #161: `spawn worker(); yield; print("m")`.
    let src = "\
.func main
    PUSH_STR \"worker\"
    SPAWN 0
    POP
    YIELD
    PUSH_STR \"m\"
    CAP_CALL \"io.print\" 1
    RET
.func worker
    PUSH_STR \"w\"
    CAP_CALL \"io.print\" 1
    RET";
    assert_eq!(run_both(src), "w\nm\n");
}

#[test]
fn engines_agree_interleaved_output_order() {
    assert_eq!(run_both(INTERLEAVED), "w1\nm1\nw2\nm2\n");
}

#[test]
fn engines_agree_main_returning_ends_program() {
    let src = "\
.func main
    PUSH_STR \"spin\"
    SPAWN 0
    POP
    YIELD
    PUSH_STR \"main done\"
    CAP_CALL \"io.print\" 1
    RET
.func spin
loop:
    YIELD
    JMP loop";
    assert_eq!(run_both(src), "main done\n");
}

#[test]
fn engines_agree_await_after_task_already_finished() {
    // The worker finishes during the YIELD; awaiting it afterwards must still
    // deliver 42 and continue (the scheduler used to block forever here and
    // end the program silently).
    let src = "\
.func main
    PUSH_STR \"work\"
    SPAWN 0
    YIELD
    AWAIT
    CAP_CALL \"io.print\" 1
    PUSH_STR \"after\"
    CAP_CALL \"io.print\" 1
    RET
.func work
    PUSH 42
    RET";
    assert_eq!(run_both(src), "42\nafter\n");
}

#[test]
fn engines_agree_spawn_args_and_await() {
    let src = "\
.func main
    PUSH 20
    PUSH 22
    PUSH_STR \"add\"
    SPAWN 2
    AWAIT
    CAP_CALL \"io.print\" 1
    RET
.func add
    ADD
    RET";
    assert_eq!(run_both(src), "42\n");
}

#[test]
fn engines_agree_time_slice_preempts_without_yield() {
    // No YIELD anywhere: main counts to 200 then prints; the worker must get a
    // turn when main's 50-instruction slice runs out, on both engines.
    let src = "\
.func main
    PUSH_STR \"worker\"
    SPAWN 0
    POP
    PUSH 0
    STORE 0
loop:
    LOAD 0
    PUSH 1
    ADD
    STORE 0
    LOAD 0
    PUSH 200
    LT
    JNZ loop
    PUSH_STR \"main\"
    CAP_CALL \"io.print\" 1
    RET
.func worker
    PUSH_STR \"worker\"
    CAP_CALL \"io.print\" 1
    RET";
    assert_eq!(run_both(src), "worker\nmain\n");
}

#[test]
fn engines_agree_spawn_of_unknown_function_is_null() {
    let src = "\
.func main
    PUSH_STR \"nope\"
    SPAWN 0
    CAP_CALL \"io.print\" 1
    RET";
    assert_eq!(run_both(src), "null\n");
}
