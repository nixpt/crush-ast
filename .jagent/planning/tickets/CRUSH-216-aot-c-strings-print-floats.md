# CRUSH-216 — AOT C: 256-byte string ring buffer truncates strings; `io.print` stack contract; float literals and formatting

| Field | Value |
|-------|-------|
| **ID** | CRUSH-216 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-11 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

1. **String storage.** All strings live in a 256-byte ring buffer
   (`crates/crush-aot/src/codegen_c.rs:~205-220`, `STRBUF_SIZE 256`; `_str_alloc` wraps),
   so long-lived strings get overwritten. Building a 3×3 board `" 0 1 2"` row by row:
   interp prints three full rows, AOT gcc's last row is ` 0 1 `. Garbles
   fifteen_puzzle, lights_out and snake under gcc and clang. Same class as CRUSH-11 (marked Done).
2. **`io.print` pushes nothing, but the CASM pops a result** (`codegen_c.rs:~1147`), so
   the caller's pending operand is popped:
   `fn f(){ print("call"); return 1 }  let a = f() + f()  print(a)` →
   AOT gcc `type error: + on non-numeric operands`; interp `2`.
3. **Floats.** `push_float` emits Rust `{v}` formatting (`codegen_c.rs:~738`):
   `100000000000000000000.0` becomes an integer literal (clang refuses; gcc prints
   `7.76628e+18`). `io.print` uses `%g`: `1` for 1.0, `0.3`, `1.5e+10`, `0.666667`, `-0`
   vs interp `1.0`, `0.30000000000000004`, `15000000000.0`.
4. `crush-aotc run` prints the return value before program output (C stdio buffered, not flushed).

## Success criteria

- [ ] Heap-allocated (refcounted or arena-per-run) strings; the board repro and the three games match interp.
- [ ] `io.print` follows the same stack contract as the VM (pushes Null).
- [ ] Float literals emitted with C syntax (`%.17g` + ensure `.0`/exponent); printing matches interp's shortest-roundtrip format.
- [ ] stdout flushed before the runner prints the result.
