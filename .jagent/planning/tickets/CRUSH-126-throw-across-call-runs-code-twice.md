# CRUSH-126 — `throw` caught across a call boundary runs code twice (handler runs in the callee's frame)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-126 |
| **Priority** | P1 |
| **Status** | Done (2026-10-04, branch `claude/crush-126-try-catch-unwind`) |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **GitHub** | [#66](https://github.com/nixpt/crush-ast/issues/66) (filed 2026-10-04 by pranix) |

## Problem

`ENTER_TRY` records only a handler IP. On `THROW` the VM jumps there **without unwinding call frames or the operand stack**, so a throw inside a callee runs the caller's catch block inside the callee's frame; the callee then returns into the caller and the post-try code runs again. Both `portable_vm.rs` (~l.939–965) and `scheduler.rs` (~l.1163–1180). CRUSH-25 documented this as a known limitation and only guarded the resulting panic.

## Reproduction

From [#66](https://github.com/nixpt/crush-ast/issues/66) (verbatim):

#### Summary

When a `throw` is caught, post-catch control flow is corrupted in two ways:

1. The statement(s) following the try/catch execute **twice**.
2. If the try block contained statements after the throwing call, they **re-execute with null locals**.

A `return` inside the catch block is ignored.

#### Repro 1: continuation runs twice

```crush
fn boom() { throw "kaboom" return 0 }
fn main() {
  print("A")
  try { boom() } catch e { print("caught") }
  print("C")
}
```

Expected: `A`, `caught`, `C`. Actual: `A`, `caught`, `C`, `C`.

#### Repro 2: try-block re-executes with nulls

```crush
fn thrower() { throw "x" return 0 }
fn main() {
  try {
    let v = thrower()
    print("v: " + v)
  } catch e { print("caught") }
  print("done")
}
```

Actual output: `caught`, `done`, `v: null`, `done` — the `print("v: " + v)` re-runs with `v=null`, then the continuation runs again. If the re-executed statement itself throws (e.g. field access on the null), the new error masks the original and escapes as "uncaught".

#### Expected

After the catch block completes, execution continues exactly once at the statement following the try/catch. `return`/`break` inside catch should be honored.

## Success criteria

- [x] both repros in the issue print exactly once, in order
- [x] `return` inside catch returns from the enclosing function
- [x] throw from a nested call 3 frames deep is caught by the right handler; uncaught still errors
- [x] same behaviour on FastVM/JIT/AOT (differential test)

## Technical approach

- Push `(handler_ip, call_depth, stack_len)` on `ENTER_TRY`; on `THROW`, pop frames to `call_depth`, truncate the operand stack to `stack_len`, push the error, jump.
- `EXIT_TRY` on normal exit and on `RET` past the try's frame must drop stale handlers.

## Files to modify

- `crates/crush-vm/src/portable_vm.rs`
- `crates/crush-vm/src/scheduler.rs`
- FastVM / JIT / AOT equivalents (check each)

## Resolution

Reproduced on `main` `a8247af` first (both repros printed exactly what #66 reports). `ENTER_TRY` now pushes a `TryHandler { handler_ip, call_depth, stack_len }` (`crush-vm/src/vm.rs`); `THROW` truncates the call and operand stacks to it before jumping, in both `scheduler.rs` and `portable_vm.rs`; `RET` drops handlers registered by the returning frame. FastVM already unwound correctly; AOT backends don't support exceptions (unchanged); JIT tests pass unchanged.

Live (`crush-run`): repro 1 → `A caught C`, repro 2 → `caught done`, plus a 3-frame / return-in-catch / return-in-try case. Tests: `crush-lang-sdk/tests/gh_issue_66_try_catch.rs`; in `crush-aot/tests/differential_aot.rs` the three-function rethrow test and a new exception-pipeline sub-test now require the interpreter and portable VM to agree with FastVM (previously FastVM-only because of this bug) — both fail without the fix.

Found while testing: the scheduler drops `main`'s return value (filed as CRUSH-137).
