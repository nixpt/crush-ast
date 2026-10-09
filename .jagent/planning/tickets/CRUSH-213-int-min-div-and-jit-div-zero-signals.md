# CRUSH-213 — `i64::MIN / -1` panics or SIGFPEs on every engine; JIT dies with a signal on `/ 0` and `% 0`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-213 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] `checked_div`/`checked_rem` (or explicit MIN/-1 check) in interp, FastVM, JIT
      and AOT codegen — one shared rule, every backend (CRUSH-114 pattern).
- [ ] JIT emits a zero-divisor check before `sdiv`/`srem` and returns a runtime error.
- [ ] Differential tests for both cases across all engines.
