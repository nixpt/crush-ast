# Handoff

Updated: 2026-08-23

## Summary

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
