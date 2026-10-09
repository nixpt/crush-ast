# State

Updated: 2026-08-23T02:00:00-05:00

crush-ast's foundation and M1 correctness spine are mostly complete, but M1 is
not closed: CRUSH-1/AI execution, nested array semantics, residual builtin and
AOT parity findings, and the remaining awesome-crush regressions are open.
M2's JIT implementation arc is substantially landed through Phases 1-5; full
conformance, optimization, and AOT-from-JIT closure remain. CRUSH-66 is done:
BUCKETS-15 merged and the `@lang[pypi:/npm:]` sandbox path was verified live.
CRUSH-119/120 are complete on the dedicated FastVM/compiler branch. Next:
close the correctness spine, then choose M2 closure, M3 debugger inspection,
or M5 AI-native VM execution.

## Update 2026-10-09

Appended rather than rewritten: the summary above dates from 2026-08-23 and has
not been re-verified here. What is true as of this update:

- A test-drive sweep on 2026-10-09 (merged as PR #124) filed CRUSH-179..221.
  CRUSH-213..221 cover places where engines give different results.
- PR #125 (branch `ccr-12236bee-uj4oar`, open, CI green) fixes the two causes
  behind GitHub #37 and #92:
  - CRUSH-187: functions that return early from a branch now get an implicit
    return.
  - CRUSH-189: the optimizer no longer makes rewrites that change results.
  It closes #37 and #92 on merge.
- awesome-crush `game_of_life` finishes in 286,738 steps. `pong` runs correctly
  but needs about 2.3M steps, more than the 1M default limit.
- New invariant: `-O` must not change what a program observably does
  (`invariants.md`).
- Known red: `cargo clippy --workspace --all-targets` fails on `3.14` test
  literals (`approx_constant`) in several crates. CI runs clippy without
  `--all-targets` and is green.
