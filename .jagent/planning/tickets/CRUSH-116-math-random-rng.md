# CRUSH-116 — Add `math.random`/`math.seed`: a real numeric RNG capability

| Field | Value |
|-------|-------|
| **ID** | CRUSH-116 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Crush has no numeric RNG primitive — confirmed by grep across
`crush-lang-sdk/src/stdlib.rs` and `crush-vm/src/caps.rs`: zero hits for
`random`/`Rng`/`rng`. Every program across `examples/crush/` and
`awesome-crush` that needs unpredictability hand-rolls its **own** linear
congruential generator from scratch — pong, the 15-puzzle's scramble,
blackjack's deck permutation, and more, each independently reinventing
`(state * a + c) % m`. This is real, repeated, evidenced friction: five+
independent implementations (by five different models/authors) of the exact
same missing primitive.

## Approach

- Add `math.random()` (returns a float in `[0, 1)`) and `math.random_int(lo,
  hi)` to `stdlib.rs`'s registry, alongside the existing `math.*` caps
  (`MathSqrtCap` etc. — same file, same macro-based pattern where
  applicable).
- Add `math.seed(n)` for **explicit, reproducible** seeding. Default to a
  fixed seed (e.g. `0`) rather than OS entropy/`thread_rng` — this
  ecosystem's whole `@covers`/example-program convention depends on
  deterministic, reproducible output (every hand-rolled LCG in this
  collection was chosen specifically for that reason), and a
  non-deterministic default would break that pattern for anyone who forgets
  to call `math.seed` explicitly.
- Back it with a small, dependency-free PRNG (a `SplitMix64` or `xorshift`
  is enough — no need for `rand`'s full dependency surface for something
  this ecosystem uses for game shuffles and scrambles, not cryptography).

## Definition of done

- [ ] `math.random`/`math.random_int`/`math.seed` registered and implemented
- [ ] Default seed is deterministic; explicit seeding documented
- [ ] `@covers` test proving same-seed → same-sequence, through the real
      pipeline
- [ ] Nice-to-have, not blocking: port one existing example (e.g.
      `blackjack.crush`'s `(seed + 17*index) % 52` affine permutation) to
      the new primitive, as a real proof of adoption

## Files to modify

- `crates/crush-lang-sdk/src/stdlib.rs` — new caps
- `crates/crush-vm/src/caps.rs` — if `math.random` should also be a portable
  (always-on) cap rather than stdlib-gated; cross-reference CRUSH-113's
  default-on decision before choosing which registry this belongs in

## Gates

None. Loosely related to CRUSH-113 (stdlib feature default-on decision) for
where this cap should live, but not blocked by it.
