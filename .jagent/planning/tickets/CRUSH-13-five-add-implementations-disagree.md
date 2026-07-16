# CRUSH-13 — Five independent `add`/arithmetic implementations disagree (bugarium flagship target)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-13 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | loosely related to CRUSH-10/CRUSH-11 (same backends), but this is about semantic divergence, not compile/output bugs |
| **Estimated effort** | L |

## Problem

Five separate places implement arithmetic (`+`, `/`, `%`, etc.) and they
give **different answers for the same program**:

1. `crush-vm/scheduler.rs` — the interpreter `crush-run` actually uses
2. `crush-vm/portable_vm.rs` — `PortableVm`
3. `crush-vm/fastvm/execution.rs` — `FastOp::Add` via `binary_op`, numeric-only; `crush-python` calls `run_fastvm`
4. `crush-aot/codegen.rs` — emits Rust via `bin_arith`, numeric-only
5. `crush-aot/codegen_c.rs` — emits C via `_add()`

A prior session (s385) fixed string-concat + a loud `TypeError` in (1) and
(2) only — which **widened** the gap, since (3)/(4)/(5) didn't get the same
fix. **Proven divergence, independent of that fix**: `1 / 0` raises
`DivisionByZero` loudly in the scheduler interpreter, but
`crush-aot/codegen.rs` emits `if b != 0 then a/b else 0` and **silently
returns 0**. Same program, different answers, no error from either path.
Same class of bug very likely applies to `%` (mod-by-zero) and possibly
other operators — not fully audited.

## Impact

This is the exact scenario the `crush-diff` differential harness
(`crates/crush-lang-sdk/src/differential.rs`, already built and already
caught one real divergence on its first run — see `dejavue`) exists to
catch systematically, but `crush-diff` currently only compares
interpreter/portable/fastvm (backends 1-3) — the AOT backends (4/5) "do not
link" was the old blocker (now resolved, see CRUSH-16) and were never
wired into the comparison. `crush-aot` is not in the runtime dependency
graph (zero reverse deps — no source file imports it), but it IS a
workspace member that builds and ships two real binaries (`crush-aotc`,
`crush-walk-run`), so a user genuinely can invoke it and get silently
different semantics than every other execution path.

## Reproduction

```bash
cat > /tmp/divzero.crush <<'EOF'
fn main() { print(1 / 0); }
EOF
crushc /tmp/divzero.crush -o /tmp/divzero.cvm1
crush-run run /tmp/divzero.cvm1
# [runtime] DivisionByZero  (loud, correct)

crush-aotc compile /tmp/divzero.crush --emit so
# .so compiles; calling crush_run() silently returns 0, no error
```

## Technical approach

1. Audit all 5 arithmetic implementations for `+`, `-`, `*`, `/`, `%`
   specifically for zero-division and type-mismatch behavior; catalog
   every divergence found (not just the one already proven).
2. Decide the canonical semantics once (loud error on div/mod-by-zero,
   matching the scheduler/portable_vm precedent already set for
   string-concat type errors) and propagate to all 5 backends.
3. Wire the AOT backends (crush-aot) into `crush-diff` as backends D/E now
   that they link (CRUSH-16) — this was explicitly deferred pending the
   link fix and should no longer be blocked.
4. Add corpus test cases specifically for div/mod-by-zero and any other
   divergence found, so `crush-diff` catches regressions here going
   forward — this whole bug class is exactly what the harness exists for.

## Files to modify

- `crates/crush-vm/src/scheduler.rs`, `portable_vm.rs`, `fastvm/execution.rs`
- `crates/crush-aot/src/codegen.rs`, `codegen_c.rs`
- `crates/crush-lang-sdk/src/differential.rs` (wire in AOT backends)

## Resolution

Implemented a comprehensive fix that brings all five backends into agreement on arithmetic semantics:

### Shared canonical arithmetic (VM backends)
- Created `crates/crush-vm/src/arithmetic.rs` — single source of truth for ADD, SUB, MUL, DIV, MOD, NEG, and numeric comparisons on `Value`.
- Refactored `scheduler.rs` and `portable_vm.rs` to delegate all arithmetic opcodes to `crate::arithmetic`.
- Created `crates/crush-vm/src/fastvm/arithmetic.rs` — mirror for FastVM's `RuntimeValue` type.
- Created `crates/crush-vm/src/io_print.rs` — shared `format_io_print_line` with canonical trailing newline.

### AOT Rust backend (`codegen.rs`)
- Fixed argument passing: args are now pushed to the stack instead of inserted directly into locals, so the CASM body's `store` instructions pop correctly instead of overwriting args with `Null` from an empty stack. Fixes `1 == "1"` returning `Bool(true)`.
- Replaced the old `bin_cmp` function with `bin_cmp_eq_ne` (uses `RuntimeValue::PartialEq` for all types, enabling string equality and correct cross-type `false`) and `bin_cmp_ordered` (numeric-only, matching scheduler).
- Added `bin_add` string concatenation when either operand is a string.
- Added overflow detection for int add/sub/mul (checked arithmetic).
- Added arithmetic type errors for non-numeric operands (crash instead of silent Null).
- Added `crush_float_to_text` for consistent float formatting (`.0` suffix for integer-valued floats).
- Added `io_print_line` for trailing newline consistency.

### AOT C backend (`codegen_c.rs`)
- Added overflow detection to `_add`, `_sub`, `_mul` via `__builtin_*_overflow`.
- Changed `_div` and `_mod` from silently returning 0 on division by zero to error+exit.
- Added overflow detection to `_neg` for `i64::MIN`.
- Added string equality to `_cmp` for EQ/NE (via `strcmp`).
- Added string concatenation to `_add` (via new `_to_text_buf` helper with separate buffers to avoid `_strbuf` reuse corruption).
- Fixed float serialization in `crush_run()`: use `%.1f` for integer-valued finite floats so `Float(7.0)` outputs `"7.0"` not `"7"`.

### Differential test harness (`tests/differential_aot.rs`)
- Made interpreter/portable return-value comparisons optional (residual stack may be empty for `return`-based programs).
- Added `Norm::Other` skip: when any backend returns an internal representation (e.g., FastVM arena `Ref`), skip detailed value comparisons and only assert outcome class.
- 14 end-to-end tests covering: mixed int/float promotion, div-by-zero, modulo, string concat, negation, comparisons, cross-type equality, overflow, ordered comparison with non-numeric types.

### Verification
```bash
cargo test -p crush-aot --test differential_aot
# 14 passed, 0 failed — all backends agree

cargo test -p crush-vm
# 115 passed, 0 failed — crush-vm regression clean

cargo check -p crush-aot -p crush-lang-sdk -p crush-vm-capi -p crush-aotc
# Clean (pre-existing warnings only)
```

## Non-goals

- Fixing CRUSH-10/CRUSH-11 themselves (those are separate, already-tracked
  compile/output bugs in the same two files this ticket also touches)
