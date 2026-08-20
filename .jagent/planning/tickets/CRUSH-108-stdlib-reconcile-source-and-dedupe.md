# CRUSH-108 — Reconcile CRUSH-56's restoration source; dedupe against already-wired stdcaps

| Field | Value |
|-------|-------|
| **ID** | CRUSH-108 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 |

## Problem

Live exploration (2026-08-20, captain's `awesome-crush` comparison arc) surfaced two
things CRUSH-56's plan doesn't currently account for:

1. **A better source may already exist, live, outside the archive.** CRUSH-56 step 1
   points at locating `exosphere-1.0.zip` and reconstructing a per-cap
   clean/mock-tainted map from it. But `exosphere` itself still carries a live,
   actively-built stdlib crate at `crates/core/base/stdlib` (28 files, ~7900 lines) —
   a genuine workspace member, depended on by `crush-lang`, `corecaps`, `exo-core`,
   `exo-cli`, `exo-vortex`, `khukuri-exo`, and `tests`. Scanning every file in it for
   `mock|todo!|unimplemented!|stub` turns up hits in exactly 2 of 28 files
   (`ai_capabilities.rs`: 7, `polyglot_bridge.rs`: 8) — a much smaller mock surface
   than CRUSH-56's archived-zip figure of 46/137. It is not yet established whether
   the zip is an older/frozen snapshot of this same live tree, a divergent fork, or
   something else — but restoring from a possibly-stale zip when a live, tested,
   currently-compiling tree sits right there (`exosphere/archive/archived-stdlib/`
   is a *third* copy, also worth diffing) risks redoing work exosphere has already
   kept current, or missing fixes the zip predates.
2. **Some of the 103/137 are already done, here, today.** `crush-ast`'s own
   `crates/crush-lang-sdk/src/stdlib.rs` already implements and wires 59 stdcaps
   (strings, math, conversion, array-only collections) as real `HostCaps` —
   zero mock markers, registered unconditionally (`//! These are stdcaps — always
   available, no capability gate required`), and directly wraps Rust's own std
   (`math_unary_cap!(MathSqrtCap, "sqrt", f64::sqrt)` etc. — same pattern
   exosphere's `math.rs` uses). Neither CRUSH-56 nor any of the CRUSH-88..97 shard
   tickets reference `crush-lang-sdk/src/stdlib.rs` at all. Partitioning the 103-cap
   archive list into shards by family (io/fs/string/net/process/...) without first
   checking it against what's already live risks a shard re-restoring caps
   (string/math/conversion/collections families) that are already done and tested.

## Approach

1. Diff `exosphere/crates/core/base/stdlib` against `exosphere/archive/archived-stdlib`
   and whatever `exosphere-1.0.zip` turns out to be (CRUSH-56 step 1) — determine
   which is actually newest/most-correct per family, and record the verdict via
   `dejavue decision`.
2. Before CRUSH-56 step 2 (shard partitioning), cross-reference the 103/137-cap list
   against `crush-lang-sdk/src/stdlib.rs`'s existing 59 registrations by name —
   mark any already-covered cap `DONE (crush-ast, pre-existing)` rather than
   assigning it to a shard.
3. Update CRUSH-56's own file with a pointer to this reconciliation before the first
   shard (CRUSH-88) is dispatched.

## Definition of done

- [ ] Verdict recorded (dejavue) on which of the 3 candidate sources (exosphere
      live tree / exosphere archived-stdlib / exosphere-1.0.zip) is authoritative
      per cap family
- [ ] The 59 already-wired `crush-lang-sdk` caps cross-checked off the 103/137 list
- [ ] CRUSH-56 updated to reference this ticket before shard dispatch begins

## Files in scope

- `crates/crush-lang-sdk/src/stdlib.rs` (crush-ast, read-only reference)
- `exosphere/crates/core/base/stdlib/`, `exosphere/archive/archived-stdlib/` (read-only reference, different repo)
- `.jagent/planning/tickets/CRUSH-56-stdlib-clean-restore-tracker.md`

## Gates

None — this can run before or alongside CRUSH-56 step 1; it changes CRUSH-56's
inputs, not its blockers.
