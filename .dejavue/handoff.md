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

CRUSH-116 is complete on `agent/nixp/CRUSH-116`. The existing dependency-free
SplitMix64 implementation is now wired as the stdlib `math.random`,
`math.random_int`, and `math.seed` capability surface. Each
`HostCapsBuilder::stdlib(true)` registry owns an independent RNG state seeded to
zero, so separate runtimes are reproducible and do not consume one another's
sequences.

`math.random()` returns a float in `[0, 1)`. `math.random_int(lo, hi)` returns an
integer in `[lo, hi)`, supports the full valid i64 span, and rejects `lo >= hi`.
`math.seed(n)` resets the sequence and returns the seed. Native capability calls
have precise semantic result types, and JavaScript `Math.random()` lowers to the
canonical `math.random` capability. The feature remains stdlib-gated; CRUSH-113
owns the default-on decision.

## Verification

- `crush-lang-sdk` direct RNG tests: deterministic default, same-seed replay,
  float/integer bounds, invalid-range rejection, full-i64-range acceptance.
- `crush-lang-sdk` source-pipeline test with `math.seed`, `math.random`, and
  `math.random_int`: passed.
- `crush-lang-js` `Math.random` lowering regression: passed.
- `cargo check -p crush-lang-sdk --features stdlib -p crush-lang-js -p crush-frontend`: passed.
- `git diff --check`: passed.
- Targeted rustfmt was inspected; workspace-wide rustfmt remains noisy due to
  unrelated formatting drift in the sibling `buckets` checkout and host process
  limits.

## Remaining work

The non-blocking adoption nice-to-have is still open: port an existing example
such as `blackjack.crush` to the new RNG. CRUSH-117 (`conv.chr`/`conv.ord`) and
CRUSH-118 (the user-facing `io.read` demo) remain separate tickets.
