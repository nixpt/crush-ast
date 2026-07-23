# CRUSH-28 — `crush-index` crate v0: ingest CAST → SQLite index + JSON export

| Field | Value |
|-------|-------|
| **ID** | CRUSH-28 |
| **Priority** | P1 — depends on CRUSH-27 (annotations must exist in CASM before index can ingest them); blocks CRUSH-29 (caps over the index) and CRUSH-31 (dejavue integration) |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-27 (annotation CAST node types must exist before the index can ingest them) |
| **Estimated effort** | L |

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section.
Implements `ai-native-roadmap.md` Step 4: "crush-index: consume CAST →
build SQLite index." This is the **data-layer** half of the navigation
layer; CRUSH-29 (the `codebase.*` host-cap provider) reads from this index.

## Problem

`ai-native-roadmap.md` describes `crush-index` as (verbatim):

> A new crate that consumes CAST from all compilation units and builds a
> cross-referenced index. Authoritative because it comes directly from the
> compiler, not from heuristic source extraction.

Today there is no `crush-index` crate under `crates/`. `codebase.*` host
caps (CRUSH-29) have nothing to query. Without an authoritative
compiler-derived index, agents rely on heuristic-source-extraction tools
(`crush-symbols` per the `crush-squad/`.jagent PROJECT.md mention) which
can drift, miss invariants, and have no `@covers` annotation to track.

The empty `crates/` slot for `crush-index` shown in `PROJECT.md`'s
workspace listing is already documented in the project's own identity doc
— this ticket fills that slot.

## Success criteria

- [ ] New `crates/crush-index/` crate created; declared in workspace
      `members`; depends on `crush-cast`, `crush-frontend`, `rusqlite`
      (or `sqlx`), and `serde_json`.
- [ ] Index schema covers the 6 entities from `ai-native-roadmap.md`:
      symbol table (name → file/line/signature/manifest), call graph
      (function → {callers, callees}), dependency graph (module →
      {imports, importers}), invariants (named invariant → {applies_to,
      reason, consequence}), coverage map (error/code path → `@covers`
      test or absence), exhaustive-match sites (type → all match-on
      sites + missing-arms detection).
- [ ] Public CLI: `crates/crush-index/src/bin/crush-index.rs` accepting
      `<file_or_dir> --emit sqlite <db>` and `--emit json <json>` modes
      (clap-based, matches workspace CLI convention).
- [ ] JSON export stable across runs (deterministic ordering, sorted keys;
      required by the M5 `@covers`-test-verified-STDLIB-restoration
      workflow — M9 dependency).
- [ ] Tests: 6 entities × 3 (build, query, roundtrip) = **18 base tests**,
      plus 4 cross-entity tests (e.g., `uncovered_paths` actually
      returning the `VmError::StackUnderflow` example from
      `docs/tasks/vm-pipeline-gaps.md`).
- [ ] `cargo test -p crush-index --lib` green; 0 leaked warnings.

## Technical approach

1. **Schema.** A SQLite schema with 6 tables (`symbols`, `calls`, `deps`,
   `invariants`, `coverage`, `match_sites`) + a `document_meta` table for
   source path/timestamp. Index keys on `(file, line)` for symbols;
   `(caller, callee)` pair for calls; etc.
2. **Ingestion.** `crush-index` exposes `pub fn build(casm_or_dir: &Path) ->
   Result<Index>` that ingests a CASM blob or a directory of `.crush`
   files and writes the SQLite database. Re-runs are idempotent
   (`INSERT OR REPLACE` keyed on `(file, line, entity_kind)`).
3. **JSON export.** Mirror of SQLite schema as serde structs;
   `crush-index export --emit json` runs the SQLite-to-JSON conversion
   and writes to stdout (deterministic: sorted keys, depth-first order).
4. **Query layer.** `pub fn query_uncovered_paths() -> Vec<UncoveredPath>`
   and similar query helpers — these are the building blocks for
   CRUSH-29's `codebase.*` cap implementations.
5. **CLI.** Use `clap` (workspace standard).

## Files to create

- `crates/crush-index/Cargo.toml`
- `crates/crush-index/src/lib.rs` (entity types + index struct)
- `crates/crush-index/src/schema.rs` (or `schema.sql`)
- `crates/crush-index/src/ingest.rs`
- `crates/crush-index/src/query.rs`
- `crates/crush-index/src/json_export.rs`
- `crates/crush-index/src/bin/crush-index.rs` (CLI entry)
- `crates/crush-index/tests/` (the 22 tests above)

## Files to modify

- `Cargo.toml` — add `crates/crush-index` to workspace `members`
- `.github/workflows/ci.yml` (or equivalent) — ensure `crush-index` is
  in `cargo test --workspace` and `cargo check --all-features`

## Non-goals

- **No incremental/online index builds in v0.** v0 is full rebuild from
  scratch. Incremental is v1, after the M5 milestone is complete.
- **No `semantic_search`.** `crush-index` doesn't include embedding
  models or natural-language query in v0. `codebase.semantic_search`
  is a separate ticket (the `ai-native-roadmap.md` "semantic_search"
  cap is wired but stubbed or missing).
- **No dejavue change-feed joining.** That's CRUSH-31.
- **No `codebase.*` cap implementation.** This ticket is the data
  layer; the cap layer is CRUSH-29.
- **No cross-repo aggregation.** v0 indexes one workspace at a
  time. Multi-repo aggregation is a separate concern (post-M5).

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 2 of 8
- `docs/design/ai-native-roadmap.md` Step 4 — the canonical spec
- `PROJECT.md` workspace tree — the `crush-index/` slot this fills
- CRUSH-27 (must precede — annotations feed the index)
- CRUSH-29 (depends on this — caps query the index)
- CRUSH-31 (depends on this — dejavue joins the index's change feed)
- M9's STDLIB restoration workflow (consumer; M9 requires this ticket)
