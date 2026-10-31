# CRUSH-295 — JIT: a function with more locals than the frame stride loses them across a call (GitHub #273)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-295 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M2 (JIT) |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | nixp — 2026-10-30, from GitHub #273 |

## Problem

On the JIT, a function with **more than 8 locals that calls another function**
had its locals from slot 8 upward overwritten by the callee's frame. The run
finished successfully with wrong values. VM, portable VM, FastVM and AOT were
all correct. (First noted, unticketed, by the CRUSH-zorro-55 work in #259.)

`crates/crush-jit/src/compiler.rs` addressed every frame as
`locals[call_stack_top * FRAME_LOCALS + idx]` with a **fixed** `FRAME_LOCALS = 8`,
and `LoadLocal`/`StoreLocal` did not bounds-check `idx`. A caller using local 8+
therefore spilled into the frame the callee was about to use, so the callee's
`store` clobbered the caller's locals.

## Success criteria

- [x] A function with more locals than the frame stride keeps them across a call.
- [x] Recursion with more locals than the stride keeps each frame's locals.
- [x] Differential tests on every engine.

## Resolution (2026-10-30, `agent/nixp/CRUSH-295`)

The frame stride is now the program's largest local count
(`program_frame_locals`: the max `LoadLocal`/`StoreLocal`/`LoadLocalChecked`
slot + 1), threaded through `build_fn` → `emit_one` → `lload`/`lstore`/
`unassign_frame`. Frames no longer overlap, so a callee cannot clobber the
caller's locals. A program whose frame does not fit the shared 64-slot `locals`
array is refused (→ FastVM fallback) rather than corrupted; the call-depth guard
becomes `JIT_MAX_LOCALS / frame_locals`.

This also makes the old `LoadLocalChecked`-past-slot-8 refusal unnecessary: a
checked load past the old stride of 8 is now an ordinary slot that reads as
unassigned, like any other.

### Evidence

- `crates/crush-aot/tests/backend_agreement.rs::many_locals_survive_a_call_on_every_engine`
  (`f` with 11 locals calling `g`) and `::recursion_with_many_locals_agrees_on_every_engine`
  (recursive `r` with 10 locals, depth 4) run on all six engines and require
  agreement. Verified RED before the fix (JIT returned `Int(55)` where every
  other engine returned `Int(74)`) and GREEN after.
- `crates/crush-jit/src/lib.rs::a_checked_load_past_the_old_frame_stride_is_a_real_slot`
  replaces the now-obsolete `a_checked_load_past_the_frame_falls_back_to_fastvm`.
- `cargo test --workspace --lib --tests`: 2541 passed, 0 failed.

## Follow-up (not this ticket)

`crates/crush-jit/src/lib.rs::test_recursive_int_sum` is still `#[ignore]`d with
a note about a CRUSH-17 Cranelift GVN issue. It passes with `--ignored` on both
the fixed and unfixed tree, so the note is stale — worth un-ignoring separately.
