# CRUSH-108 — test_cmp_eq_nan_never_equal_jit fails deterministically on main

**Status**: Backlog · **Priority**: P1 (red test on main = gate noise for every merge)

## Problem

`crush-jit` lib test `test_cmp_eq_nan_never_equal_jit` (assert at
crush-jit/src/lib.rs:638) fails deterministically (3/3 runs) — verified
PRE-EXISTING at `060d9c5` (before the s412 CRUSH-71/factory merges) in a
clean worktree. Either the JIT compares NaN == NaN as true (real IEEE-754
miscompile — same wrong-answer class as CRUSH-72) or the test's expectation
is wrong. CI presumably shows green — check whether CI even runs `crush-jit
--lib` and whether the warm-cache trap (CRUSH-CI-CACHE-1) hides it.

## Done

- [ ] Root cause: JIT NaN semantics vs test expectation, decided + fixed
- [ ] Test green 20/20; CI actually exercises it (evidence)
