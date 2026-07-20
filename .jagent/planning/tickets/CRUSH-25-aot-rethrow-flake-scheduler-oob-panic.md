# CRUSH-25 — AOT rethrow differential test flake: scheduler `code[ip]` index-out-of-bounds panic

| Field | Value |
|-------|-------|
| **ID** | CRUSH-25 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M2 (JIT/VM backends) |
| **Assignee** | sangam |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`crates/crush-aot/tests/differential_aot.rs::aot_rethrow_through_three_functions_agrees_fastvm`
was flagged s389 (during CRUSHVM-EQ-1) as "aot rethrow flake (pre-existing,
filed)" but no ticket was actually created — this ticket closes that gap
(dispatched as CRUSH-AOT-RETHROW-1, s394).

Reproduced against `main` `a17d1f1`: running the standalone test binary
50x (single test, single process each run, no parallelism) failed **14/50**
(28%), always with the identical signature:

```
thread 'aot_rethrow_through_three_functions_agrees_fastvm' panicked at crates/crush-vm/src/scheduler.rs:397:52:
index out of bounds: the len is 64 but the index is 64
```

## Root cause

Two independent, compounding issues — only the first was in scope to fix
here; the second is a pre-existing, already-documented limitation this
ticket does NOT touch.

1. **The actual panic (fixed by this ticket).** `crush_vm::scheduler::run`'s
   dispatch loop indexes `code[ip]` (scheduler.rs:397, pre-fix) with no
   bounds check — unlike `crush_vm::portable_vm::Vm::step` (portable_vm.rs:279),
   which already guards `if ip >= code.len() { return Err(TruncatedInstruction) }`
   before touching `code[ip]`. `scheduler.rs` was missing the equivalent
   guard, so whenever a thread's `ip` ran exactly to (or past) `code.len()`,
   the interpreter panicked instead of returning a normal `VmError` — which
   aborts the whole test process (`differential_run` calls
   `crush_vm::run(&vm_prog, &quotas)` unconditionally, even though this
   specific test only reads `fastvm_return()`).

2. **Why `ip` runs off the end at all (documented pre-existing limitation,
   NOT fixed here — matches the comment already in the test file).** The
   scheduler's `THROW` handler (scheduler.rs `THROW =>`) pops a handler ip
   off the thread's flat `try_stack` and jumps to it directly, but does
   **not** unwind the thread's `call_stack` back to the depth it was at
   when the matching `ENTER_TRY` ran. In `a() { try { b() } catch e {...} }`
   → `b() { c() }` → `c() { throw 7 }`, by the time `c`'s `THROW` fires,
   `call_stack` has two live frames (the `a→b` and `b→c` calls) that the
   jump to `a`'s catch handler never pops. Those stale frames' `return_ip`
   values later get popped by unrelated `RET`s (e.g. `main`'s), sending
   `ip` to addresses that are only valid relative to *some* other function's
   position in the compiled code blob — landing anywhere from "still inside
   a function, wrong answer" to "off the end of the 64-byte blob entirely".

   **Why this is a *flake* and not a deterministic 100%-repro bug:**
   `casm::Program::functions` and `crush_cast::Program::functions` are both
   `HashMap<String, Function>` (crates/casm/src/lib.rs:530,
   crates/crush-cast/src/lib.rs:32). `crush_lang_sdk::compile::casm_to_vm`
   iterates `&program.functions` (compile.rs:225) to lay each function's
   code into the final byte blob — so which of `main`/`a`/`b`/`c` lands at
   which byte offset is randomized per-process (Rust's default `HashMap`
   hasher is seeded once at process start, stable within a run, random
   across runs). Whether a stale frame's `return_ip` happens to still land
   inside valid code, land on a wrong-but-in-bounds instruction, or run
   past `code.len()` depends entirely on that random layout — hence ~28%
   of process invocations hit the exact layout that panics, the rest don't.

   This class of bug (flat `try_stack` not unwinding `call_stack` across
   function-call boundaries during multi-function throw) is **already
   called out in the test's own doc comment** ("The scheduler (interpreter)
   and portable VM have a pre-existing limitation... This test therefore
   only validates the FastVM result directly.") — i.e. it was a known,
   accepted, in-scope-elsewhere gap before this ticket. Fixing it properly
   (unwinding `call_stack` to the `ENTER_TRY`-time depth on `THROW`, for
   both `scheduler.rs` and `portable_vm.rs`) is real work with its own
   blast radius and is explicitly **out of scope** for this ticket — see
   Non-goals.

## Fix (this ticket)

`crates/crush-vm/src/scheduler.rs` — added a bounds check identical in
shape and error to `portable_vm.rs`'s existing guard, before the first
`code[ip]` dereference in the dispatch loop:

```rust
if ip >= n {
    return Err(VmError::TruncatedInstruction(ip));
}
let isize = bytecode::instruction_size(code[ip]).ok_or(VmError::UnknownOpcode(code[ip], ip))?;
```

This does not change the underlying call_stack-unwind bug (issue 2, still
present — the interpreter/portable VM can still return a *wrong* value for
multi-function rethrow across some random layouts, exactly as the test's
existing comment already discloses and already scopes around). What it
does eliminate is the **panic**: `ip` running past `code.len()` now always
surfaces as a normal `VmError::TruncatedInstruction`, caught by
`stack_outcome()` into `StackOutcome::Err(_)`, so `differential_run` returns
`Ok(DiffReport{ interpreter: Err(_), .. })` instead of the whole test
process aborting. Since this specific test only asserts on
`report.fastvm_return()`, that's enough to make the flake go away
completely — the interpreter's wrong/erroring result for this program was
never part of this test's assertion surface.

**Why this makes the panic deterministic-impossible, not just less
likely:** the guard is unconditional and precedes every `code[ip]` read in
the loop — there is no code path left that can reach `code[ip]` with
`ip >= code.len()`. It doesn't matter what random HashMap layout the
function bodies land in; `ip >= n` is now caught before the indexing
operation exists, full stop.

## Success criteria

- [x] Reproduced with iteration counts (50 runs, standalone process each,
      no parallelism): 14/50 failed, single identical panic signature.
- [x] Root-caused via instrumentation (read the actual panic line + the
      sibling `portable_vm.rs` guard that scheduler.rs was missing — not
      theorized from the suspect list in the dispatch prompt).
- [x] Fix applied; `code[ip]` can no longer be reached with an out-of-range
      `ip` in `scheduler.rs::run`.
- [x] Post-fix: 200/200 clean on the specific test (0 failures, 0 panics),
      50/50 clean on the full `differential_aot` suite (22/22 passing every
      run).
- [x] Full 4-suite gate green: `crush-vm --lib` 128/128 (default features)
      + 128/128 (`--features python`, needs
      `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` on this box's py3.14 venv) ·
      `crush-jit --lib` 79/79 (includes its own
      `test_throw_unwind_three_functions_with_rethrow_from_handler`,
      passing deterministically — JIT's call/try-stack design doesn't share
      this bug) · `crush-aot` (full package: lib + `differential_aot` +
      `integration` + `integration_c`) all green, 0 failed.

## Files modified

- `crates/crush-vm/src/scheduler.rs` — added `if ip >= n { return
  Err(VmError::TruncatedInstruction(ip)); }` immediately before the first
  `code[ip]` dereference in the dispatch loop, mirroring the existing guard
  in `portable_vm.rs::Vm::step`.

## Non-goals

- **Not fixing the call_stack-unwind-across-function-boundary bug itself**
  (issue 2 above). The interpreter and portable VM can still compute a
  wrong return value (not just avoid a panic) for multi-function
  rethrow-through-catch programs, depending on function layout — this is
  the same pre-existing, already-documented, already-scoped-around
  limitation the test file's own comment describes. A real fix needs
  `THROW` to walk `call_stack` back to the depth recorded at the matching
  `ENTER_TRY` (both `scheduler.rs` and `portable_vm.rs`), which is a larger
  and riskier change than a flake-elimination ticket should carry. Filing
  this as a separate concern for whoever next touches VM exception
  semantics — worth a dedicated ticket if/when the interpreter/portable VM
  backends need to agree with FastVM on multi-function throw, not just
  avoid crashing.
- Not touching `crush-jit`'s multi-function rethrow path — already correct
  (`test_throw_unwind_three_functions_with_rethrow_from_handler`,
  `test_throw_unwind_through_three_functions_to_middle_handler` both pass
  deterministically, confirmed in this ticket's gate run).
- Not making `program.functions` iteration order deterministic (e.g.
  switching to `BTreeMap` or an insertion-ordered map) — that would also
  incidentally hide issue 2's flakiness by pinning it to one layout, but
  doesn't fix the actual bug and wasn't needed once the panic itself is
  gone; not pursued here to keep this ticket's diff minimal and honest
  about what it actually fixes.
