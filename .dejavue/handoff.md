# Handoff

Updated: 2026-08-23

## Summary
CRUSH-119 is complete on `agent/buffy/CRUSH-119-for-loop`. FastVM now has
CVM1-parity contracts for array mutation: `push`/`append` preserve the mutated
array reference, `pop` returns the array plus removed value, and `arr_set`
returns the mutated array. Compiler-emitted array primitives (`push`, `append`,
`pop`, `arr_get`/`index`, `arr_set`, `arr_len`) lower to native FastOps instead
of requiring an unavailable host capability.

CRUSH-120 is also complete on the same dedicated branch. The frontend
optimizer now removes an assignment target from its constant map before
rewriting the RHS. This preserves the runtime value in self-referential
updates such as `total = total + item`, including loop-carried accumulators.

The planning records were reconciled on 2026-08-23. M0 is now explicit as
foundation/release hygiene; M1 is marked mostly complete with residual
correctness findings; M2 is marked substantially implemented through Phases
1-5 with conformance, optimization, and AOT-from-JIT closure still open; M3
and M4 remain partial; M5 is partial/active; M6-M11 remain proposed. CRUSH-66
is recorded as done and its obsolete BUCKETS-15 blocker was removed.

## Verification
- `CARGO_BUILD_JOBS=1 cargo test -p crush-frontend --lib`: 83 passed.
- `CARGO_BUILD_JOBS=1 cargo test -p crush-lang-sdk --lib`: 181 passed.
- Focused FastVM arithmetic regression: passed, returning `6` for `[1, 2, 3]`.
- `cargo check -p crush-vm -p crush-lang-sdk`: previously passed for CRUSH-119.
- `git diff --check`: passed before final metadata edits.
- Targeted rustfmt check on changed Rust files: passed for CRUSH-119; workspace-wide
  formatting remains blocked by unrelated pre-existing drift.
- `crush-vm` built with one job, but its full test harness hit the environment's
  process/thread resource limit while starting/listing tests; no code failure
  was reported.

## Known follow-up
The compiler and optimizer are covered by the new loop accumulator regression.
Existing repository warnings remain unrelated to CRUSH-120. Remaining roadmap
closure work is listed in `.jagent/planning/ROADMAP.md` and `.jagent/planning/TASKS.md`.

## Next Steps
1. Foreman reviews and merges the CRUSH-119/CRUSH-120 branch.
2. Close the M1 correctness spine or choose M2 conformance/AOT closure, M3
   debugger inspection, or M5 AI-native VM execution.
3. Keep M6-M11 gated on their documented dependencies.

## Boot Instructions
Read `.dejavue/handoff.md`, `.dejavue/state.md`, `.dejavue/decisions.md`, and
`.dejavue/timeline.jsonl` before making changes.
