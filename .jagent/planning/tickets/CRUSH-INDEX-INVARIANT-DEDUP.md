| Field | Value |
|-------|-------|
| **ID** | CRUSH-INDEX-INVARIANT-DEDUP |
| **Priority** | P2 — narrower blast radius than CRUSH-28's Module dedup; effect is duplicate rows in `idx.annotations()` output, not data inconsistency |
| **Status** | Done |

## Resolution

Closed by [`2f61c44`](https://github.com/nixpt/crush-ast/commit/2f61c44) (feat(crush-index): CRUSH-INDEX-INVARIANT-DEDUP -- invariant.name first-write-wins).

Implemented in `crates/crush-index/src/index.rs::CrushIndex::annotations()` + 3 regression tests in `crates/crush-index/src/tests.rs`. Singleton-dedupe key for `Annotation::Invariant` is `.name` (first-write-wins via `HashSet::insert`'s bool return), mirroring the established Module-per-`module_path` dedup shape from CRUSH-28. Wildcard arm `_ => true` preserves function-level variant stacking so cross-module coverage closure (`@covers` in `tests` closing `@errors` in `impl`) stays intact.
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-28 (annotation flat ladder in crush-cast) [DONE — commit `88633b1`] |
| **Estimated effort** | Small — 1 file, ~30 lines, 1-2 unit tests |

## Origin

Filed from a SHOULD-FIX note in CRUSH-28's code-reviewer pass (Nit Pick Nick): the doc-comment on `CrushIndex::annotations()` calls out that `Annotation::Invariant` dedup-by-`.name` is "a known gap, filed for the next turn." This ticket IS that next turn. The Module dedup landed alongside CRUSH-28's flat-ladder work; this ticket ships the parallel Invariant dedup.

## Problem

`CrushIndex::annotations(module_path)` reads the per-module flat ladder cache (`flat_annotations: HashMap<String, Vec<Vec<Annotation>>>`) and returns a merged, deterministically-sorted list. The post-read step at the end of `annotations()`:

```rust
let mut seen_module = false;
out.retain(|ann| match ann {
    Annotation::Module(_) if seen_module => false,
    Annotation::Module(_) => { seen_module = true; true }
    _ => true,
});
```

keeps only the first `Annotation::Module` per module_path; other variants
stack freely. The intended semantics for `Annotation::Invariant` is
parallel — each Invariant is keyed by `name`, and re-ingesting the
same `module_path` with a different `manifest.invariants` Vec would
currently stack duplicates by name. Three concrete failure modes today:

1. **`codebase.invariants("mod")`** (registered in CRUSH-29, commit
   `e4302ad`) returns N rows per Invariant when the module is
   re-ingested N times — the cap reads the underlying
   `idx.invariants(module)` map, which is already populated by
   `manifest.invariants.clone()` and so is unaffected by the stack; but
   the FLAT-LADDER view (which the cap doesn't read yet) accumulates
   duplicates silently.
2. **`idx.annotations("mod")`** (the unifying primitive from CRUSH-28)
   returns N copies of the same logical invariant.
3. A future `codebase.modules()` entry that nests an `invariants`
   array (ticket spec calls for
   `[{name, reason, consequence, applies_to}]`) would emit duplicate
   rows in the same way Module dedup (CRUSH-28) prevented duplicate
   Module rows. Pre-emptive dedup here keeps the FLAT-LADDER surface
   clean for any future consumer that exposes it.

## Solution

Mirror the Module dedup pattern with a parallel `Annotation::Invariant`
dedup. Key choice: invariant `name` is the natural unique key — it's
mandatory, non-empty by validation, and the only field guaranteed to
distinguish two `Invariant` entries at the source.

**Approach:**

1. In `CrushIndex::annotations(module_path)`, after the existing
   `seen_module` retain, add a parallel `seen_invariant_names: HashSet<String>`
   retain — drop the second-and-later Invariant entries whose `name`
   is already in the set. First-write-wins (same semantic as Module
   dedup, documented in the existing `annotations()` doc-comment).
2. Update the doc-comment on `CrushIndex::annotations()` to mark the
   Invariant-dedup follow-up as RESOLVED — delete the prior
   "filed for the next turn" sentence and add a one-line reference to
   this ticket ID.
3. Add a regression test in `crates/crush-index/tests.rs`: mirror
   `test_annotations_module_dedup_across_add_program_calls` but for
   `Invariant` — two `add_program()` calls with the same `module_path`
   but different `manifest.invariants` Vecs asserting ONE
   `Annotation::Invariant` row per unique `.name`.

**Edge cases:**

- Two `Invariant`s with the same `name` and DIFFERENT
  `description`/`applies_to`: today both surface; after the fix, only
  the first is kept. First-write-wins semantics means: the OLDER
  `add_program()` wins for invariant content (same shape as Module).
- Two `Invariant`s with DIFFERENT names: both are kept. The dedup is
  per-name, not global.
- Pure `Invariant::name == ""` (malformed): dropped after the first
  occurrence. Iterator visits `""`-named Invariants after sort (they
  tie under alphabetical order); only the first survives. Mirrors
  Module's `(0, "")` tie-break being a singleton.
- `Invariant::name` collision from a normalize/normalize-fold pipeline
  that lowercases names: out of scope for this turn — the dedup
  assumes strict byte equality on `name`. If a normalize layer lands,
  it should be a separate ticket that runs BEFORE `Annotation::Invariant`
  is built, not on the dedup side.

## Non-Goals

- **Last-write-wins** semantics: not pursuing. First-write-wins is
  consistent with the Module dedup pattern (and the user's framing in
  the parent followup: "narrower blast radius" — they explicitly want
  the parallel shape). If priority later shifts to "latest manifest
  wins", a separate ticket `CRUSH-INDEX-INVARIANT-LASTWRITE` can land.
- **Cross-module invariant dedup**: invariants are module-scoped;
  cross-module dedup would conflate legitimately-distinct invariants
  with similar names ("no-reenter" in scheduler vs "no-reenter" in
  vm.types may have different purposes). NOT in scope.
- **Source-location attachment to `Invariant`**: separate ticket
  `CRUSH-29-EXTEND-LOCS` (the codebase.* caps `file`/`line` shape gap).
- **Renaming `description` → `reason`** on `Invariant`: separate
  ticket `CRUSH-INVARIANT-TERM-SPLIT` (raised in CRUSH-29 review).

## Success Criteria

1. `cargo test -p crush-index` passes with a new regression test
   asserting `Annotation::Invariant` is reduced to one-per-`name` per
   `module_path`, even after multiple `add_program()` calls.
2. All 25 existing `crates/crush-index/tests::tests::*` tests still
   pass (regression check — the dedup is purely additive, no field
   renames, no field removals).
3. The doc-comment on `CrushIndex::annotations()` no longer contains
   the "filed for the next turn" note for Invariant dedup; instead,
   it cross-references this ticket ID and notes the first-write-wins
   semantic once that becomes the actual behavior.
4. No regression in `cargo build -p crush-lang-sdk` — `codebase.invariants()`
   reads `idx.invariants(module)` directly (not the flat ladder), so
   this fix targets: (a) the FLAT-LADDER surface that `codebase.modules()`
   and any future inline-`invariants` consumer will use, AND (b) any
   direct `idx.annotations()` consumer (none in production today, but
   the unification primitive stays clean for CRUSH-29 follow-ups).

## Implementation Sequence

1. `crates/crush-index/tests.rs`: write the regression test FIRST.
2. `crates/crush-index/src/index.rs`: add the
   `seen_invariant_names: HashSet<String>` retain in `annotations()` and
   update the doc-comment.
3. Run `cargo test -p crush-index` to confirm green.
4. Update ticket Status → Done, commit on
   `agent/buffy/M2-JIT-PHASES-2-4` (or current branch).

## Tests

- New test: `test_annotations_invariant_dedup_across_add_program_calls`
  — two `add_program()` calls with same `module_path`, two
  `manifest.invariants` Vecs containing ONE same-named entry each
  (`name: "inv-1"`), assert `idx.annotations("mod").filter(Invariant).count() == 1`.
- New test: `test_annotations_invariant_kept_when_names_differ` — two
  `add_program()` calls with same `module_path`, two DIFFERENT-named
  Invariants (`name: "inv-1"` and `name: "inv-2"`), assert both
  Invariants surface.
- (Optional) New test: `test_annotations_invariant_dedup_first_write_wins`
  — assert that when same-named entry appears in the SECOND
  `add_program()` call, the FIRST `add_program()`'s entry's
  `description` survives (and the second's is silently dropped). This
  locks in the first-write-wins semantic.
