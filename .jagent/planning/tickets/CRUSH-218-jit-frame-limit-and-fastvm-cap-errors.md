# CRUSH-218 — JIT allows only 8 call frames; JIT float `%` int fails; FastVM turns failed caps into strings

| Field | Value |
|-------|-------|
| **ID** | CRUSH-218 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- JIT: `crates/crush-jit/src/compiler.rs:~999` (`JIT_MAX_LOCALS 64 / FRAME_LOCALS 8`)
  → recursion depth 8. `f(8)` gives generic `JIT execution error (flag=1)`, no depth
  message, no FastVM fallback. Breaks fib(10), blackjack, fifteen_puzzle, lights_out,
  breakout and fibonacci under JIT.
- JIT `print(7.5 % 2)` → `null` + `flag=1`; `7.5 % 2.0` works.
- FastVM: a failed capability call becomes a string value and execution continues
  (`crates/crush-vm/src/fastvm/execution.rs:~165-171`, marked TODO). JIT reports
  `Capability call failed`; interp fails.
- JIT `JIT stack overflow` panic at `crates/crush-jit/src/runtime.rs:~1406` on the
  CRUSH-187 fall-through shape (a panic, not an error).

## Success criteria

- [ ] JIT frame stack sized like the interp's call-depth quota, with a proper depth error or fallback.
- [ ] Mixed int/float `%` in JIT.
- [ ] FastVM propagates cap errors as runtime errors.
- [ ] No JIT path panics; errors become runtime errors.
