# CRUSH-223 — AOT C: fixed-size value stack and array pool fail silently or with the wrong error

| Field | Value |
|-------|-------|
| **ID** | CRUSH-223 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (related: CRUSH-216, CRUSH-219) |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 AOT test drive, reproduced on `main` `5755262` (debug build) |

## Problem

`crates/crush-aot/src/codegen_c.rs` runtime:

- **Value stack, 512 slots.** `_push` does `if (_sp < STACK_MAX) _stack[_sp++] = v;` and
  `_pop` returns `null` on empty, so an overflow drops values and a later op reads
  `null`:
  ```crush
  fn sum(n) { if n == 0 { return 0; } return n + sum(n - 1); }
  fn main() { io.print(sum(600)); return 0; }
  ```
  AOT gcc: `crush(aotc): type error: + on non-numeric operands`. interp: call depth
  quota exceeded (256). Any program can hit it with deep enough expressions/recursion.
- **Array pool.** Arrays are never reclaimed, so `a = a + [i]` in a loop stops at
  ~2000 iterations with `crush(aotc): array pool exhausted` (interp: `2000`).

## Success criteria

- [ ] Stack overflow is a clear runtime error (or the stack grows); never silently drops a value.
- [ ] Arrays are reclaimed or growable; the `a = a + [i]` loop runs to 10,000.
- [ ] Both cases covered by a test that compares with interp.
