# CRUSH-30 — `@exhaustive-match-sites` compiler lint (tracks all match sites for a type + flags missing arms)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-30 |
| **Priority** | P1 — depends on CRUSH-27 (annotation must parse/compile-emit); M5 milestone; the compile-time arm of the coverage story |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-27 (parse + compiler emit for `@exhaustive-match-sites TypeName`); the lint reads build-time data only (does not depend on CRUSH-28) |
| **Estimated effort** | M |

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section —
implements `ai-native-roadmap.md` Step 7: "`@exhaustive-match-sites`
compiler tracking + warning."

## Problem

`ai-native-roadmap.md` Example (verbatim):

```crush
// Sum types track all exhaustive-match sites
@exhaustive-match-sites Value
```

Today this annotation doesn't exist; `crush-frontend`'s compiler has no
cross-file match-tracking pass; there is no way to know in advance which
files match on `Value` and whether they're complete. Adding a new variant
to `Value` requires `grep` + manual cross-check of every match site
(`vm.rs`, `scheduler.rs`, `portable_vm.rs`, `bus.rs`, `caps.rs`,
`stdlib.rs`, plus 12 walker crates — the existing pattern surfaces in
ROOT_SYSTEM_POLICY's "Exhaustive match discovery" root-cause #5).

## Success criteria

- [ ] Parser recognizes `@exhaustive-match-sites TypeName` per CRUSH-27
      (the annotation's CAST node type ships via CRUSH-27's `Annotation`
      enum).
- [ ] Compiler pass `crush_frontend::compiler::match_site_tracker`
      walks the whole workspace's CAST after each compilation unit and
      records every `match`/`if let` expression where `TypeName` is the
      scrutinee; snapshot result into a side artifact (`.exhaustive.json`
      next to CASM, or round-trip back into the index for CRUSH-28 to
      ingest).
- [ ] Compiler emits a `WARN` (not error — opt-in via the annotation)
      when a tracked type's match-site list is incomplete or has
      possibly-missing arms. The heuristic is **best-effort**:
      type-system-inference based, not a sound guarantee — accepted
      as v1 per the ai-native-roadmap spec ("agents read this before
      touching the module").
- [ ] Diagnostic id `W-EXM01` added to the existing crush-diagnostics
      series (per the workspace's `crush-diagnostics` crate conventions).
- [ ] **3 integration tests**: (a) `@exhaustive-match-sites Value` on a
      2-variant type, 2 files matching, no warning; (b) add a third
      variant and 1 new match site, no warning; (c) add a fourth variant
      but no new match site, `W-EXM01` fires.

## Technical approach

1. **Tracker.** New `crush-frontend/src/compiler/match_site_tracker.rs`
   module: walks the discovered CAST files (via the workspace index built
   from `crush-frontend`'s own file walker — re-uses the existing
   pattern, doesn't depend on `crush-index`) and builds a
   `HashMap<TypeName, Vec<MatchSite>>` table.
2. **Warning emission.** During CASM emission, for every type covered by
   an `@exhaustive-match-sites` annotation, query the tracker and emit a
   `WARN` if the tracker's type-coverage heuristic detects unmached
   variants. The heuristic compares the union of pattern types observed
   in match sites vs. the inferred variants of the type via the
   type-system — best-effort.
3. **Diagnostic id.** Adds `W-EXM01` to the existing crush-diagnostics
   series.
4. **Tests.** Workspace-scale test using a fixture workspace with
   intentional gaps; check that the warning fires only on case (c).

## Files to modify

- `crates/crush-frontend/src/compiler/mod.rs` — invoke tracker
- `crates/crush-frontend/src/compiler/match_site_tracker.rs` (new)
- `crates/crush-frontend/Cargo.toml` — add `walkdir` dep if needed
- `crates/crush-diagnostics/src/lib.rs` — register `W-EXM01`

## Non-goals

- **Not a type-sound guarantee.** Warning is best-effort; intentional
  false positives are accepted for v1. This is a lint, not a checker;
  it supplements — doesn't replace — Rust's own exhaustive-match check
  on the underlying match expressions.
- **Doesn't replace Rust's `match` exhaustiveness on `TypeName` itself.**
  The compiler is still in charge of arm-completeness for type-correct
  Rust; this annotation is for **crate-level** completeness tracking only.
- **No IDE integration.** Warning shows in `crushc` output and
  `crush-notebook` runtime diagnostics; no separate editor hook yet.

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 4 of 8
- `docs/design/ai-native-roadmap.md` Step 7
- CRUSH-27 (annotation parser/compiler emit — prerequisite)
- CRUSH-36 (LanguageAdapter migration, post-M5 — lint precision
  benefits from a unified trait; not required for v1)
