# CRUSH-31 — dejavue ↔ crush-index integration: change-feed joins over annotation-graph

| Field | Value |
|-------|-------|
| **ID** | CRUSH-31 |
| **Priority** | P2 — depends on CRUSH-28 (index data layer) + CRUSH-27 (annotation); blocks M9's STDLIB-restoration `@covers` verification gate |
| **Status** | Superseded |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-27 (annotation data structures) + CRUSH-28 (index data layer) + CRUSH-29 (caps surface to extend with `codebase.annotation_history`) + existing dejavue timelines (`.dejavue/timeline.jsonl`, `.dejavue/decisions.md`) |
| **Estimated effort** | M |

> Superseded s412: canonical file is `CRUSH-31-dejavue-integration.md` (work landed: 431a88f + a773c8d).

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section —
implements `ai-native-roadmap.md` Step 8: "dejavue ↔ crush-index
integration (change feed)."

## Problem

dejavue tracks **why** decisions were made (architectural memory per its
purpose statement on `github.com/nixpt/dejavue`); `crush-index` tracks
**what** code paths and annotations exist (structural/semantic memory).
They're disjoint today — a `crush-notebook` agent that wants to ask
"what changed in this annotation since the last decision was logged?"
requires manually stitching them.

Without integration, **M9's STDLIB restoration workflow cannot use
`@covers` tests as a verified gate**: the test needs to be linked to
both the source file and the historical change that introduced it,
and that linkage doesn't exist today. Other M9 dependencies also rely
on this — `crush-notebook` AI cells (M5 thesis) want the change-feed
join to answer "which annotation is currently stale w.r.t. last
logged decision?"

## Success criteria

- [ ] `crush-index` `ingest` step reads `.dejavue/timeline.jsonl` and
      `.dejavue/decisions.md` alongside CASM/CAS and writes:
      - link table `annotation_event(uri) -> dejavue_decision(event_id)`
      - per-annotation historical-context table (which DEJAVUE entry
        introduced the annotation, changeset timestamp for each
        annotation update)
- [ ] `codebase.annotation_history("module.purpose")` returns the
      ordered chain of DEJAVUE entries touching that annotation. This
      is CRUSH-29's `codebase.*` family extension — implemented as a
      follow-up to CRUSH-29 in this ticket so the full M5 integration
      test is in one place (the deliverable listed under `ai-native-roadmap.md`
      Step 6 family).
- [ ] M9 STDLIB restoration workflow can answer: "for cap `foo`, which
      `@covers` test was added by which commit, did that commit follow
      the no-mock-marker discipline" — directly queryable via the
      linked table (verified by an integration test that points at
      the 103 clean-restore candidates from `.dejavue/decisions.md`'s
      STDLIB RESTORATION MAP).
- [ ] Tests: (a) round-trip integration test ingesting a sample repo
      + sample dejavue timeline; (b) `annotation_history` returns the
      correct chain for a known-annotated file; (c) M9 STDLIB mock-marker
      discipline query returns the expected verdicts on a fixture
      manifest.

## Technical approach

1. **Schema extension.** Add 2 tables to `crush-index`'s SQLite:
   `dejavue_events` (event_id, ts, agent, decision_title, decision_reason_summary)
   and `annotation_event_links` (annotation_uri, event_id, link_kind).
   `link_kind` ∈ {`introduced`, `modified`, `removed`, `touched_by_review`}.
2. **Ingest step.** `crush-index ingest` accepts `--with-dejavue
   <path>`; reads `timeline.jsonl` (line-delimited JSON, established
   per `.dejavue/timeline.jsonl` shape) and `decisions.md` (markdown
   headings). Don't write a dejavue parser — parse only the fields
   `crush-index` needs (event_id, ts, decision_title) and file a
   separate CRUSH-NN ticket upstream for shared dejavue parsing if
   the parser is non-trivial.
3. **`annotation_history` query.** New `pub fn annotation_history`
   in `crush-index::query` — returns ordered list of events
   touching a given annotation URI, indexed by ts.
4. **`codebase.*` family extension.** Extend CRUSH-29's caps with
   `codebase.annotation_history(...)` provider — added to the same
   M5 integration test.
5. **M9 test fixture.** Build a fixture manifest of 5 cleanly-restored
   caps + 2 mock-tainted caps; verify `annotation_history` discipline
   query returns the expected `pass` / `fail` per cap.

## Files to modify

- `crates/crush-index/src/ingest.rs` — dejavue path
- `crates/crush-index/src/schema.rs` (or inline) — new tables
- `crates/crush-index/src/query.rs` — `annotation_history` function
- `crates/crush-lang-sdk/src/codebase_caps.rs` — new cap impl

## Non-goals

- **Doesn't replace dejavue.** dejavue stays its own tool; this is a
  *read-side* integration. dejavue writes are unchanged. The ticket
  reads existing timeline/decisions files; it does not write to them.
- **No reverse direction.** This ticket indexes dejavue *into*
  `crush-index`; it doesn't write dejavue entries from `crush-index`
  changes. (That direction is a future CRUSH-NN, post-M5.)
- **No shared dejavue parser.** This ticket parses the few fields it
  needs; doesn't introduce a new dependency on dejavue's own parsers.

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 5 of 8
- `docs/design/ai-native-roadmap.md` Step 8
- CRUSH-28 (the index — prerequisite for dejavue ingest)
- CRUSH-27 (annotation data structures)
- CRUSH-29 (the cap layer — receives the new `annotation_history` cap)
- M9's `STDLIB restoration` workflow (the consumer gate; M9 is blocked
  on this ticket being live before STDLIB work begins)
- `.dejavue/timeline.jsonl` and `.dejavue/decisions.md` — the source
  data this ticket reads (dejavue v0 schema confirmed in
  `nixpt/dejavue` upstream)
