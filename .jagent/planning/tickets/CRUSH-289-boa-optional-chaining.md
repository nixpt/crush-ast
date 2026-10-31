# CRUSH-289 — boa walker: optional chaining (`?.`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-289 |
| **Priority** | P1 |
| **Status** | Done — PR #294 merged 2026-10-29 |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | muse |
| **Filed by** | muse — 2026-10-21, follow-on to CRUSH-288 |

## Problem

Optional chaining refuses outright ("optional chaining (`?.`)").
It is maya's first blocker in engine/core/Component.js,
games/biomes.js and games/race.js, in shapes like
`this.owner?.get(type)`, `this.onAttach?.()`,
`g.parent?.remove(g)`, `race?.surface === 'vacuum'`.

## Design

boa flattens a whole chain into one `Optional` node: a target
plus operations, each marked *shorted* (`?.`) or not (`.`).
The walker folds the chain over the boa AST it already lowers:
each shorted operation becomes
`__crush_ternary__(prefix == null, null, rest-of-chain)` — the
prefix compared with the walker's loose equality (JS `== null`
matches both `null` and `undefined`, exactly the `?.` trigger),
the remainder lowered only on the non-null branch, so call
arguments on a shorted call never evaluate. Non-shorted
operations apply through the ordinary access/call lowering, so
class members, proven Map/Set receivers (CRUSH-288), `.length`
and the rest keep their meaning inside chains.

The prefix is evaluated twice (once for the test, once in the
branch), so a shorted operation whose prefix can have side
effects — a call result, an assignment, an update — refuses
loudly instead of running twice (`a?.b()?.c`,
`f()?.x` stay loud; the same conservatism as compound
assignment to a computed target). Maya's receivers are names
and member chains, which are pure by the walker's existing
`has_side_effects` rule.

Out of scope (stay loud): private fields in a chain
(`a?.#x`), side-effecting prefixes as above, and `?.` in
assignment targets (not valid JS).

## Evidence (2026-10-21)

- Implementation: `optional_chain` in `lower/expr.rs` folds
  boa's flattened chain over synthesized plain expressions; a
  shorted operation emits
  `__crush_ternary__(js.loose_eq(prefix, null), null, rest)`.
  The loop pre-scan (`assigned_syms_expr`) learned the
  `Optional` node. Two supporting fixes:
  - a plain binding whose checker type is concrete (the
    initializer's type: `const n = null` is `Null`) is read
    back through `[n][0]` in the branch — the same value typed
    `Any` — since the checker refuses a field read on a
    `Null`-typed target even in the unreachable branch.
    Kind/host-tracked bindings and member chains are not
    wrapped (their reads are `Any`, and their lowering is
    receiver-sensitive);
  - `js_core::loose_eq` tests null first (as the C AOT runtime
    already did): a function value compared with null is now
    `false` instead of an error, which the null test over a
    method-valued prefix (`this.onAttach?.()`) needs.
- Tests: `test_boa_optional_chaining_run` (missing links,
  proven-Map receivers, `this.owner?.lookup(t)`, a subclass
  hook `this.onAttach?.()`, arguments of a shorted call never
  evaluating, chains inside `==`/ternary/`||`) and
  `test_boa_optional_chaining_loud` (side-effecting prefixes,
  private fields). The `ops_optional_chain` corpus program now
  observes the short-circuit through `== null` (the corpus's
  undefined idiom; it printed `undefined`, which the walker's
  merged null cannot print) and moves to BOA_FIXED / AOT
  BOA_EXACT. An SDK polyglot test that used `a?.c` as its
  canonical un-lowerable construct now uses `f()?.c`.
- Suites: standard + walkers + clippy (`-D clippy::never_loop`)
  green.
- Maya sweep (throwaway, 260 files): walked count unchanged at
  15/260 (cross-module imports/`new` still dominate), but the
  three `?.`-blocked files advance — engine/core/Component.js
  to the `.get()` row claim over member receivers (the
  CRUSH-285-review-#7 question; see the zorro-44 dispatch
  design, PR #290), games/biomes.js to a cross-module name
  (CRUSH-110), games/race.js to string methods (zorro-14).

## Follow-ups found on the way

- `===`/`!==` never compile through the checked path (filed
  as CRUSH-290).
- `this.owner?.get(type)` (Component.js) still refuses: the
  builtin-row first claim over member receivers captures
  `.get` even when the receiver is provably not a Map. Plain
  `this.owner.get(t)` refuses identically today, so chains are
  no worse than plain calls; resolving it is the zorro-44
  dispatch question.
- An optional call of a method that exists nowhere
  (`this.onAttach?.()` with no definition in the module)
  compiles-refuses like a plain call; JS would skip it.

## Survey 2026-10-31 (status)

PR #294 (`agent/muse/CRUSH-289`) merged 2026-10-29. Previous status: `In PR (stacked on CRUSH-288)`.
