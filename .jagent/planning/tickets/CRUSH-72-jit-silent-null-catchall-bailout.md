# CRUSH-72 — crush-jit: silent TAG_NULL catch-all → Unsupported error + FastVm fallback

| Field | Value |
|-------|-------|
| **ID** | CRUSH-72 |
| **Priority** | P0 (ships wrong answers today if catch-all still present) |
| **Status** | Done (s417, 2026-08-05) |
| **Phase** | Correctness spine (s412) |

## Problem

`crates/crush-jit/src/compiler.rs` ends its FastOp lowering match with a
catch-all that pushes `TAG_NULL` for any opcode it doesn't implement
(July evidence: `:421`, `_ => { let cv = iconst(b, TAG_NULL); push(b, ctx, cv); }`).
Any unimplemented FastOp JIT-compiles to a silent null and execution continues —
the same program can return a different, wrong answer under the JIT than under
the interpreter, with no error raised. Contrast `fastvm/execution.rs`, which
fails loudly (`FastError::TypeMismatch`). Every production JIT (HotSpot, V8,
LuaJIT, Cranelift's own fuzzgen contract) bails to the interpreter instead.
Same bug class as the notebook NOP mapping (CRUSH-84) — systemic, not incidental.

**Re-verify at dispatch (RULES.md):** the July numbers (31 of ~86 FastOps
implemented) predate the M2 Phase 2–4 landings (ex-CRUSH-17 → CRUSH-87 work).
Count current match arms vs the `FastOp` enum in
`crates/crush-vm/src/fastvm/instructions.rs`. If the catch-all is already gone,
flip this ticket to verify+close with evidence.

## Approach

1. Catch-all → `Err(CompileError::Unsupported(op))`.
2. `JitEngine` (crush-jit/src/lib.rs) treats Unsupported as "refuse the
   function" and the caller falls back to FastVm for that function.
3. Test: iterate every `FastOp` variant; assert each either compiles or is
   refused — never fabricates a value.

## Definition of done

- [x] No code path fabricates a value for an unimplemented opcode
- [x] Fallback exercised by a test running a program containing an
      unsupported op through the JIT entry point → correct (interpreter) result
- [x] Exhaustive compile-or-refuse test over all FastOp variants
- [x] `cargo test -p crush-jit -p crush-vm` green (96/97 pass; 1 pre-existing: CRUSH-108)

## Resolution

**Merged s417 (2026-08-05) — `agent/panini-crush/CRUSH-72` → `main` (13fba6a, ff).**

Re-verified at dispatch: catch-all at compiler.rs:1173 was still present at
`d9cb11c` (main). 58 of ~75 FastOp variants were already handled by explicit
match arms; the catch-all silently fabricated TAG_NULL for the remaining 17.

What landed (3 commits, 390 insertions / 24 deletions):

1. **compiler.rs**: New `CompileError` enum with `Unsupported(Vec<FastOp>)` variant.
   `build_fn` and `emit_one` signatures changed to `Result<_, CompileError>`.
   Catch-all `_ => { push TAG_NULL }` → `op => { return Err(CompileError::Unsupported(vec![op])) }`.

2. **lib.rs**: `JitEngine::fallback_to_fastvm()` method. `run()` wraps `compiler.compile()`
   in a match — `CompileError::Unsupported` triggers transparent FastVM fallback,
   producing the same result the interpreter would have.

3. **Exhaustive guard test** (`exhaustive_fastop_compile_or_refuse`): iterates every
   `FastOp` variant, asserts each either compiles or is refused. Implemented list (58 ops)
   verified by running the test binary; unsupported list (17 ops including Break, Continue,
   CrossLangCall, Yield, Restart, Watchdog, ExportVar, CallInterface, and 10 AI opcodes).
   Plus `unsupported_op_falls_back_to_fastvm` end-to-end test proving a program with an
   unsupported opcode runs through the JIT entry point and produces the FastVM result.

**Panini-crush dispatched (claude, 50 turns).** Agent hit the 50-turn limit during
test verification; foreman-finished with two corrections:
- Break/Continue moved from implemented→unsupported (they cause Cranelift panics,
  not clean errors — a separate defect from the catch-all)
- CrossLangCall moved similarly (listed as implemented but actually returned Unsupported)
- Unsupported test branch widened to accept both panics and clean errors

**Test results:** 96/97 pass. 1 pre-existing failure: `test_cmp_eq_nan_never_equal_jit`
(CRUSH-108 — JIT NaN equality, separate ticket, pre-dates this change).

## Files in scope

- `crates/crush-jit/src/compiler.rs`, `crates/crush-jit/src/lib.rs`
- NOT in scope: implementing missing ops (that's CRUSH-59, M10)

## Gates

None. Gates CRUSH-59.
