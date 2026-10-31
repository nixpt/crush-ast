# CRUSH-213 — `i64::MIN / -1` panics or SIGFPEs on every engine; JIT dies with a signal on `/ 0` and `% 0`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-213 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

```crush
fn id(x) { return x }
let m = id(0 - 9223372036854775807) - 1
print(m / id(-1))       // also m % id(-1)
```

- interp: Rust panic at `crates/crush-vm/src/arithmetic.rs:~119` (`/`) and `~138` (`%`).
- FastVM: panic at `crates/crush-vm/src/fastvm/arithmetic.rs:~118`/`~135`; JIT falls back to FastVM and panics there.
- AOT C: SIGFPE (rc 136).

JIT on division by zero: `let z = 0  print(10 / z)` → SIGILL (rc 132); `10 % z` → SIGFPE
(rc 136). A surrounding `try` doesn't help and prior output is lost. FastVM gives
`DivisionByZero`, interp `[runtime] division by zero`. JIT `1.0 / 0.0` prints `inf`
while every other engine errors (pick one float story — see CRUSH-185).

## Success criteria

- [x] `checked_div`/`checked_rem` (or explicit MIN/-1 check) in interp, FastVM, JIT
      and AOT codegen — one shared rule, every backend (CRUSH-114 pattern).
- [x] JIT emits a zero-divisor check before `sdiv`/`srem` and returns a runtime error.
- [x] Differential tests for both cases across all engines.

## Resolution (2026-10-30, `agent/nixp/CRUSH-213`)

The AOT Rust and C backends already had the rule (`checked_div`/`checked_rem`;
explicit `b.i == 0` and `INT64_MIN && -1` checks). The remaining gaps were the
interpreter, FastVM, and the JIT:

- **Interp** (`crates/crush-vm/src/arithmetic.rs`): the int `/` and `%` paths
  now use `checked_div`/`checked_rem`, so `i64::MIN / -1` (and `% -1`) raise
  `ArithmeticOverflow` instead of panicking in Rust.
- **FastVM** (`crates/crush-vm/src/fastvm/arithmetic.rs`): the same, raising
  `FastError::ArithmeticOverflow`.
- **JIT** (`crates/crush-jit`): `Div` and `Mod` now guard the divisor before
  `sdiv`/`srem` (which trap in hardware — SIGILL/SIGFPE) and before `fdiv`
  (which would silently yield `inf` for `1.0 / 0.0`). Two new `ctx.error`
  codes, `ERR_DIV_ZERO` and `ERR_ARITH_OVERFLOW`, are mapped to the
  "division by zero" / "arithmetic overflow" text every other engine uses; the
  JIT's existing 48-bit payload check still reports the overflow for its own
  int range.

### Evidence

- `crates/crush-aot/tests/backend_agreement.rs::division_edge_cases_agree_across_every_engine`
  runs six programs (`i64::MIN / -1`, `% -1`, int `/ 0`, `% 0`, float `/ 0.0`,
  `% 0.0`) on all six engines and requires each to *fail* with the right text.
  Verified RED before the fix: the interp/FastVM panic (`attempt to divide
  with overflow`) and the JIT process dies with SIGILL. The JIT legitimately
  falls back to FastVM for `i64::MIN` (outside its 48-bit int payload), which
  the test tolerates.
- Live: `crush-run` prints `[runtime] arithmetic overflow` for `i64::MIN / -1`
  and `[runtime] division by zero` for `10 / 0` and `1.0 / 0.0`.
- `cargo test --workspace --lib --tests`: 2540 passed, 0 failed. No new clippy
  warnings.
