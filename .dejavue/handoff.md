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

## CRUSH-117 handoff

Implemented `conv.chr` and `conv.ord` as always-on portable capabilities.

### Contract

- `conv.chr(int)` returns a Unicode scalar string and rejects negative,
  out-of-range, and surrogate codepoints.
- `conv.ord(string)` returns the scalar codepoint and rejects empty or
  multi-scalar strings.
- Scheduler and PortableVM share the same Unicode-scalar behavior and semantic
  result types (`String`/`Int`).

### Backend coverage

- Rust AOT emits and uses conversion helpers.
- AOT C emits UTF-8 conversion helpers and cap dispatch.
- AOT-C emits the corresponding runtime helpers and cap calls.
- VM direct and PortableVM tests cover ASCII/non-ASCII round trips and invalid
  inputs.
- C AOT integration covers a GCC-compiled Unicode round trip.

Focused verification passed:

- `cargo test -p crush-vm cap_conv -- --nocapture`
- `cargo test -p crush-vm test_portable_conv_chr_ord -- --nocapture`
- `cargo check -p crush-aot`
- `cargo test -p crush-aot rust_aot -- --nocapture`
- `cargo test -p crush-aot --test integration_c test_c_gcc -- --nocapture`
- `cargo test -p crush-aotc test_emit_conv_caps -- --nocapture`
- `git diff --check`

Workspace-wide rustfmt remains blocked by unrelated formatting drift in the
sibling `buckets` repository. Porting the brainfuck ASCII lookup table to use
`conv.chr` remains optional and is intentionally out of scope.

| What | Where |
|------|--------|
| CRUSH-66 ticket | `.jagent/planning/tickets/CRUSH-66-lang-deps-pypi-npm.md` |
| Design | `docs/design/lang-deps-pypi-npm.md` |
| Sandbox wiring | `crates/crush-vm/src/bucket_exec.rs` |
| crush-pkg runners | `crates/crush-pkg/src/runners.rs` |

## CRUSH-187 / CRUSH-189 handoff (2026-10-09)

PR #125, branch `ccr-12236bee-uj4oar`: open, CI green, waiting on review/merge.
Closes GitHub #37 and #92.

### What changed

- `crates/crush-frontend/src/compiler.rs` (`ensure_return`): appends
  `push_null; ret` unless the last instruction is `ret`/`throw`/`jmp`/`halt`
  AND no jump targets the end of the body. Previously a function with any
  `ret` got none, so an early return from a branch let execution fall into the
  next function.
- `crates/crush-frontend/src/optimizer.rs`:
  - Removed the identity and strength-reduction rewrites (`x*0`, `x*1`, `x+0`,
    `2*x`→`x+x` and the mirrored forms).
  - Int folding is checked: on overflow the expression is left unfolded so the
    runtime reports it.
  - Variables assigned in a `try` body are dropped from the `catch` handler's
    constants.
- Decisions and rejected alternatives are in `decisions.md`. The new rule is in
  `invariants.md`.

### Verification

- `crates/crush-lang-sdk/tests/implicit_return_and_optimizer_test.rs`: 10
  end-to-end tests. All fail without the fix and pass with it.
- `cargo test --workspace`: 1732 passed. `cargo clippy --workspace` (the CI form)
  is clean.
- Live runs:
  - #37's repro prints 0..5.
  - awesome-crush `game_of_life` finishes in 286,738 steps (it previously went
    past 50M).
  - `pong` reaches `draw 0-0` in 2,303,450 steps.
- All 34 compilable `examples/crush/*.crush` print identical output with and
  without `crushc -O`. This was run by hand.

### Next steps

1. Review and merge PR #125.
2. Turn the with/without `-O` comparison into a CI test. No ticket yet.
3. Fix the `approx_constant` failures under `cargo clippy --all-targets`, or drop
   `--all-targets` from CLAUDE.md's build commands.
4. Decide on pong's step count: document `--max-steps` as breakout does, or
   revisit the 1M default.
5. Continue with CRUSH-179..221 from the 2026-10-09 sweep (engine divergence in
   CRUSH-213..221).
