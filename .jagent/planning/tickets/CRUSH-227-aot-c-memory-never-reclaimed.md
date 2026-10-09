# CRUSH-227 — AOT C: strings, arrays and maps are never freed during a run

| Field | Value |
|-------|-------|
| **ID** | CRUSH-227 |
| **Priority** | P1 |
| **Status** | Done (2026-10-09) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | CRUSH-216, CRUSH-223 |
| **Estimated effort** | M |
| **Filed by** | claude — PR #126 review follow-up, 2026-10-09 |

## Problem

Since CRUSH-216/223 every C-backend string is a heap block and arrays/maps come from
pools, all freed only when the next `crush_run()` starts. Nothing is reclaimed during
a run, so memory is the sum of every intermediate value (review:
https://github.com/nixpt/crush-ast/pull/126#discussion_r4226979481).

Measured (peak RSS, `crush-aot-runner`, debug build):

| Program | VM | C AOT | Rust AOT |
|---|---|---|---|
| `a = a + [i]` × 5,000 | 16 MB | 214 MB | 11 MB |
| `s = s + "x"` × 20,000 | 17 MB | 199 MB | 11 MB |
| temp string per iteration × 200,000 | 16 MB | 33 MB | 11 MB |
| temp array per iteration × 300,000 | 16 MB | 53 MB | 11 MB |
| pong, breakout, snake, game_of_life | ~17 MB | ~11 MB | ~11 MB |

Accumulation is O(n²) memory (10,000 appends ≈ 800 MB; a 100,000-char string ≈ 5 GB);
temporaries grow linearly, and a loop that creates arrays stops with
`array pool exhausted` after 2^20 of them. The Rust backend and the VM refcount.

## Approach

A mark-and-sweep collector in the C runtime, run only at safepoints (top of each
function's dispatch loop, between instructions). There every live value is on the value
stack or in a locals frame — a `call` keeps its arguments on the stack and holds nothing
in C variables — so those are the whole root set. Needs:

- locals moved from C variables to a collector-visible heap stack (`_locals`, one frame
  per call; two `return` sites restore it);
- heap strings identified by binary search in the (sorted at collection) block registry,
  so string-producing code doesn't change; array/map mark bits and free lists;
- allocation only counts bytes; threshold `max(1 MB, 2 × live)`;
- `CRUSH_GC_STRESS=1` collects at every safepoint, to catch a missed root in tests.

Prototype (~190 lines in `codegen_c.rs`): every case above stays at 11–15 MB, `a = a + [i]`
× 5,000 goes from 1.26 s to 0.02 s, and all 23 C-compilable examples plus the crush-aot
suites give identical output with `CRUSH_GC_STRESS=1`.

Invariant it adds: no instruction may hold a value in a C variable across a call back
into Crush code (true today; a future `map(arr, fn)`-style op must keep its values on the
stack). No string op may return an interior pointer (all copy today).

## Success criteria

- [x] The table above stays under ~20 MB for every row (measured 11–15 MB peak RSS);
      `a = a + [i]` × 10,000 and a 100,000-char string build run under a test-enforced
      bound. (The bound is a 256 MB address-space limit via `prlimit`, not 64 MB RSS:
      the runner alone reserves ~100 MB of address space. On `main`'s runtime both die
      with `out of memory` under it.)
- [x] `stdout_parity` and `examples_parity` also run with `CRUSH_GC_STRESS=1` in CI.
- [x] No pool-exhaustion error for a loop creating 2^21 short-lived arrays.

## Resolution

- Collector in `crates/crush-aot/src/codegen_c.rs` as described above.
- Tests: `crates/crush-aot/tests/memory_bounds.rs` (three cases, both backends, under
  `prlimit --as=256M`; all three fail on the previous runtime: two `out of memory`,
  one `array pool exhausted`).
- CI: new **Test (aot)** job. Before it, CI never ran crush-aot's tests: Test
  (workspace) only compiles them (`--no-run`) and no other job includes the crate. Its
  second step reruns stdout_parity, backend_agreement, examples_parity and
  integration_c with `CRUSH_GC_STRESS=1`.
