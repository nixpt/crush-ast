# CRUSH-223 — AOT C: fixed-size value stack and array pool fail silently or with the wrong error

| Field | Value |
|-------|-------|
| **ID** | CRUSH-223 |
| **Priority** | P1 |
| **Status** | Done (2026-10-09, PR #126) |
| **Phase** | M1 |
| **Assignee** | claude |
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

- [x] Stack overflow is a clear runtime error (or the stack grows); never silently drops a value.
- [x] Arrays are growable; the pool holds 1M arrays and running out is an error. (The
      10,000-iteration `a = a + [i]` loop works but copies ~800 MB, since nothing is
      reclaimed during a run; reclamation needs refcounting, not in scope.)
- [x] Both cases covered by a test that compares with interp.

## Resolution

- Correction to the problem statement: the pool was **64 arrays** of a fixed 65,536
  slots each (64 MB static), and **32 objects**. `new_array`/`new_object` pushed `null`
  when the pool ran out; `push` and `make_range` stopped adding at the cap with no error.
- **Value stack:** heap, doubling on demand (`_stack_grow`); `_dup` goes through `_push`.
- **Arrays:** a 1M-entry header pool (untouched slots cost nothing); each array's data is
  a growable heap block (`_array_reserve`, `_array_push`) used by concatenation, `push`,
  `make_range`, `vec_add` and `matmul`. `_alloc_array` never returns -1: exhaustion is
  `array pool exhausted`.
- **Objects:** pool raised to 65,536; exhaustion and a 17th field are runtime errors.
- Tests: `crates/crush-aot/tests/stdout_parity.rs` `deep_recursion_keeps_every_stack_value`,
  `many_and_large_arrays`, `many_maps` (Rust and C backends; all three fail before).
