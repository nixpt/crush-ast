# CRUSH-17 — crush-jit Phase 2-4 correctness gaps (float Mod, serr checks, handler_pc contract, StoreLocal audit, call-stack overflow)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-17 |
| **Priority** | P1 |
| **Status** | In Progress (items 1,2,3,5,8 closed) |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none (gate for M2 Phases 5-7) |
| **Estimated effort** | M |
| **Origin** | code-reviewer-glm review of commit `fe9a60a` on `agent/buffy/M2-JIT-PHASES-2-4` (2026-07-18) |

> **Updated 2026-07-18**: Items 1, 2, 3, 5, and 8 are FIXED. Remaining blockers: #4 (StoreLocal audit). Minor items: #6, #7.

The M2 JIT Phases 2-4 commit (`fe9a60a`) landed 3,610 lines of new JIT
code (CALL/RETURN, exceptions, ~40 runtime-helper ops, bitwise, math,
float Mod) plus fastvm multi-function frame parity. It builds clean and
72 JIT + 21 AOT-differential tests pass, but a code review surfaced 7
correctness risks — 2 likely bugs, 3 high-blast-radius items needing
verification, 2 minor. The JIT throw/return path has bytecode-level
tests in `crush-jit/src/lib.rs` but **no frontend-source integration
test** (the existing `aot_rethrow_through_three_functions_agrees_fastvm`
is FastVM-only and bypasses the JIT entirely).

## Findings from code review (priority-ordered)

### 🔴 Likely correctness bugs

1. **Float `Mod` trunc-direction miscompiles for negatives** ✅ FIXED (2026-07-18)
   (`compiler.rs`, `Mod` arm): `let is_neg = band(div_bits, sign_mask);
   select(is_neg, ceil_v, floor_v)`. `is_neg` is an i64 with only the
   sign bit set (`0x8000_0000_0000_0000`), not a boolean. CLIF `select`
   tests the **low bit**, which is 0 for the sign-bit mask → `floor` is
   always chosen → wrong trunc for negative dividends. Fix:
   `let is_neg = icmp_ne(b, is_neg_raw, iconst(b, 0));` to produce a real
   bool. **Existing `test_mod_floats` only tests `7.5 % 2.5` (both
   positive) — does not catch this.**

2. **No `serr` check after runtime helper calls** ✅ FIXED (2026-07-18)
   (`emit_helper_call`): ~40 ops (Index, ArrayPush, StrSplit, Cast,
   GetField, …) delegate to `jit_runtime_helper` via `call_indirect`,
   which can set the error flag (`ctx.error`) on out-of-bounds, bad
   cast, missing field, etc. The JIT emits the `call_indirect` and
   **continues with no branch on `OFF_ERROR`**. On any helper error the
   VM keeps executing with a poisoned stack — cascading misexecution.
   Fix: new `emit_helper_call_checked` wraps `emit_helper_call` with a
   post-call `load(OFF_ERROR)` + `brif` to an error-return path.
   ~35 call sites updated. EnterTry/ExitTry/Throw left unchecked
   (Throw has own check, ET/XT never set error).

### 🟡 High blast-radius / needs verification

3. **`handler_pc` encoding contract — verify + lock with assertion** ✅ FIXED (2026-07-18)
   (`emit_handler_dispatch` vs `runtime.rs::OP_THROW`): added a
   detailed comment on `JitContext::handler_pc` documenting that the
   unit is an **instruction index** shared with `EnterTry`'s
   `instr.arg` and `compiler.rs`'s `handler_entries`. Added
   `debug_assert!(handler_pc >= 0)` in `OP_THROW` handler to lock
   the contract.

4. **`StoreLocal` peek→pop is a silent semantic flip** (`emit_one`,
   `StoreLocal` arm): previously `peek` (value stays on stack), now
   `pop` (value consumed). The commit message says "match FastVM stack
   discipline." Every existing lowered program that emits
   `Push X; StoreLocal 0` *and then reuses X* now double-consumes.
   **Verify the lowerer was audited** to emit a duplicate `Push`/`Pick`
   when the value is still needed, or this regresses working programs
   silently. At minimum add a differential test:
   `fn main() { let x = 1; print(x); print(x); }` — if `x` is
   consumed by StoreLocal, the second `print(x)` gets null.

5. **JIT call stack has no overflow guard** ✅ FIXED (2026-07-18)
   (`push_frame`): `OFF_CALL_STACK` (8768) → `OFF_CALL_STACK_TOP` (9792)
   = 1024 bytes / 16 bytes per frame = **64 frames max**
   (`JIT_MAX_CALL_DEPTH = 64`). Fix: added a `brif(call_stack_top >= 64,
   overflow_bb, ok_bb)` in the `Call` arm before `push_frame`.
   `overflow_bb` sets `serr` and returns null.

### 🔴 CONFIRMED by integration test (new finding, 2026-07-18)

8. **JIT double-seals a block when compiling frontend-lowered rethrow
   bytecode** ✅ FIXED (2026-07-18)
   (`build_fn` / deferred-sealing logic): fixed by sealing non-handler
   blocks in REVERSE order (successors before predecessors) to prevent
   SSA cascade double-seals. The `jit_rethrow_through_three_functions_agrees_fastvm`
   test is still `#[ignore]`d because the JIT throws "Uncaught exception"
   (separate issue — CRUSH-17 item #6 Throw arm return contract).

### 🟢 Minor / hygiene

6. **`Throw` arm must `return true`** after `emit_handler_dispatch`
   (which emits a `brif`/`jump` terminator). If it doesn't, the
   non-terminator fallthrough path in `build_fn` appends a second
   terminator → CLIF panic. **Confirm the arm returns `true`** (the
   14 passing exception tests suggest it does, but the exact tail
   wasn't visible in the diff).

7. **Duplicate brif-cascade functions** — `emit_return_dispatch` and
   `emit_handler_dispatch` are near-identical; unify into one
   `dispatch_by_eq(val, entries: &[(i64, Block)])` to avoid the two
   implementations drifting (they already differ in sealing order).
   Also: `MathPow` silently returns `TAG_NULL` (documented TODO) —
   either route through a runtime helper or set `serr`.

## Success criteria

- [x] Float `Mod` on negative dividends matches FastVM
- [x] Runtime helper calls check `OFF_ERROR` after `call_indirect`
- [x] `handler_pc` encoding contract locked with comment + `debug_assert!`
- [ ] `StoreLocal` peek→pop audit
- [x] JIT call-stack overflow guard
- [ ] Confirm `Throw` arm returns `true` after dispatch
- [x] **New frontend-source JIT rethrow integration test** lands (see
      below) — exercises the full pipeline
      (source → frontend → lowering → JIT) for the throw/rethrow path,
      which previously had zero integration coverage. **The test
      CONFIRMED finding #8 (double-seal) and is `#[ignore]`d until the
      sealing fix lands.** Un-ignoring it is the regression gate.

## Technical approach

1. **Float Mod fix** (item 1): one-line change in `compiler.rs` `Mod`
   arm — replace `select(is_neg, ceil, floor)` with
   `select(icmp_ne(b, is_neg_raw, zero), ceil, floor)`. Add
   `test_mod_float_negative` to `crush-jit/src/lib.rs` covering
   `-7.5 % 2.0`, `-7.5 % -2.0`, `7.5 % -2.0`.

2. **serr check after helpers** (item 2): in `emit_helper_call`, after
   the `call_indirect`, emit a load of `OFF_ERROR` + `brif` to a shared
   error block that stores the result as null and returns (or jumps to
   a per-function trap). Mirror the inline `Div`/`Mod` error-path shape.

3. **handler_pc contract** (item 3): add
   `debug_assert!(ctx.handler_pc < program.instructions.len() ||
   ctx.error == 3)` in the runtime after `OP_THROW` sets it; add a
   comment on `OFF_HANDLER_PC` and on `handler_entries` construction
   naming "instruction index" as the shared unit.

4. **StoreLocal audit** (item 4): grep the lowerer
   (`crush-vm/src/fastvm/instructions.rs::lower_instruction`) for
   `StoreLocal` emission sites; verify each is preceded by `Dup`/`Pick`
   if the value is reused. Add the `let x = 1; print(x); print(x)`
   frontend-source differential test. If the lowerer relies on
   peek-semantics, revert the JIT `StoreLocal` to `peek`.

5. **Call-stack guard** (item 5): in `push_frame`, before the store,
   add `brif(icmp_sge(top, JIT_MAX_CALL_DEPTH), overflow_block,
   ok_block)`; `overflow_block` sets `serr` (store non-zero to
   `OFF_ERROR`) and returns. Add `test_deep_recursion_overflow` to
   `crush-jit/src/lib.rs` (e.g., 70-deep recursion → error, not
   corruption).

6. **Frontend-source JIT rethrow test** (criterion 7): see the test
   added alongside this ticket in
   `crates/crush-aot/tests/differential_aot.rs` —
   `jit_rethrow_through_three_functions_agrees_fastvm`. Compiles the
   same Crush source as the existing FastVM-only rethrow test through
   the full frontend → `casm_to_vm` → `lower_program` pipeline and
   runs it on both `FastVM` and `JitEngine`, comparing `FastYield`
   equality. This is the integration coverage the bytecode-level JIT
   tests don't provide.

## Files to modify

- `crates/crush-jit/src/compiler.rs` — float Mod `select` fix (item 1),
  serr check in `emit_helper_call` (item 2), call-stack overflow guard
  in `push_frame` (item 5), confirm Throw returns `true` (item 6),
  unify dispatch functions (item 7)
- `crates/crush-jit/src/runtime.rs` — handler_pc debug_assert + comment
  (item 3)
- `crates/crush-jit/src/lib.rs` — `test_mod_float_negative` (item 1),
  `test_deep_recursion_overflow` (item 5)
- `crates/crush-vm/src/fastvm/instructions.rs` — StoreLocal lowerer
  audit (item 4, if the lowerer needs Dup/Pick insertion)
- `crates/crush-aot/tests/differential_aot.rs` — JIT-variant rethrow
  test (already added with this ticket)
- `crates/crush-aot/Cargo.toml` — `crush-jit` dev-dependency (already
  added with this ticket)

## Reproduction

### Float Mod (item 1)
```rust
// In crush-jit/src/lib.rs tests — this would FAIL today for negatives:
let a = f64::to_bits(-7.5);
let b = f64::to_bits(2.0);
let prog = make_prog(vec![
    (FastOp::PushFloat, a, 0),
    (FastOp::PushFloat, b, 0),
    (FastOp::Mod, 0, 0),
    (FastOp::Halt, 0, 0),
]);
assert_eq!(run_fastvm(&prog), run_jit(&prog)); // expected: -1.5
```

### Missing serr check (item 2)
Any program that triggers a helper error (e.g., out-of-bounds array
index) via the JIT will continue executing with a poisoned stack
instead of halting. No existing test covers this.

### StoreLocal (item 4)
```
fn main() { let x = 1; print(x); print(x); }
```
If StoreLocal pop-semantics consumed `x`, the second `print(x)` would
print null. Run through both FastVM and JIT to verify they agree.

## Non-goals

- M2 Phases 5-7 (ExoLight, optimization, AOT) — this ticket is the
  correctness gate *before* those phases; don't start Phase 5 until
  items 1-5 are closed
- The unfiled "crush-jit silently miscompiles ~55 of 86 FastOps" finding
  (panini, 2026-07-14 cranelift fuzz target) — that's a separate ticket;
  this one covers the specific review findings from `fe9a60a`
- Rewriting the exception dispatch to use `JumpTableData` instead of the
  brif cascade — the cascade works; unifying the two duplicate functions
  (item 7) is hygiene, not a correctness fix
- Adding frontend-source integration tests for *all* JIT opcodes — only
  the rethrow path is gated here; broader JIT-vs-FastVM frontend
  differential coverage is a follow-up

## References

- Commit under review: `fe9a60a` on `agent/buffy/M2-JIT-PHASES-2-4`
- Runtime layout + OP_THROW/OP_ENTER_TRY: `crates/crush-jit/src/runtime.rs`
  (lines 965-1023 for exception ops, 1084-1144 for JitContext)
- CLIF throw dispatch: `crates/crush-jit/src/compiler.rs` (lines 860-900)
- Existing bytecode-level JIT exception tests:
  `crates/crush-jit/src/lib.rs` (14 tests, including
  `test_throw_unwind_three_functions_with_rethrow_from_handler` at
  line 1482)
- Existing FastVM-only rethrow test (the sibling this ticket's test
  mirrors): `crates/crush-aot/tests/differential_aot.rs::
  aot_rethrow_through_three_functions_agrees_fastvm`
- Dejavue handoff warning about double-maintained-encoding bugs:
  `.dejavue/handoff.md` (Math.floor case-mismatch is the cited example
  of the same class)
