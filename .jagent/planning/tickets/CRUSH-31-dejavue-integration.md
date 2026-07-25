| Field | Value |
|-------|-------|
| **ID** | CRUSH-31 |
| **Priority** | P1 — foundational for M5; unblocks M9 STDLIB-restoration `@covers` verification gate and the `crush-notebook` AI cells (per ROADMAP "Step 8 of M5") |
| **Status** | Done |
| **Phase** | M5 (Step 8) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-27 (DONE — flat annotation ladder types), CRUSH-28 (DONE — `crush-index` slice including `flatten_annotations()`), CRUSH-29 (DONE — `codebase.*` host cap surface) |
| **External** | `.dejavue/timeline.jsonl` (NDJSON event stream) |
| **Downstream** | M9 STDLIB-restoration `@covers` verification gate; `crush-notebook` AI cells |

| Field | Value |
|-------|-------|
| **Commit hash** | (lands on `agent/buffy/M2-JIT-PHASES-2-4`) |
| **Files touched** | `crates/crush-index/src/dejavue.rs` (NEW), `crates/crush-index/src/index.rs`, `crates/crush-index/src/lib.rs`, `crates/crush-index/tests/dejavue_e2e.rs` (NEW), `crates/crush-lang-sdk/src/codebase.rs`, `crates/crush-lang-sdk/tests/codebase_caps_integration.rs` |

## Origin

Step 8 of the M5 AI-native compiler layer: integrate dejavue (decisions
+ project timeline) with `crush-index` (structural/semantic data) via
change-feed joins over the annotation graph. Without this layer,
`crush-notebook`'s AI-cell cap and the M9 STDLIB-restoration workflow
have no way to ask "why was this annotation written this way" — an
agent's only option today is to grep the docs by hand.

## Problem (resolved by this commit)

`.dejavue/timeline.jsonl` is a long-running NDJSON event stream
recording architectural decisions, file changes, init events, etc.
CRUSH-31 makes those events queryable against the `crush-index`
annotation graph — specifically `Annotation::Invariant.name` is the
natural join key against `event.decision_title` (the canonical decision
title string used across the project since the workspace dep migration
in 2026-06).

Before CRUSH-31: `CrushIndex::dejavue_timeline()` returned the timeline
as raw `Vec<String>` (one NDJSON line each). Agents had to write their
own serde parser inline. `annotation_history` query didn't exist.

After CRUSH-31: `CrushIndex::dejavue_events()` returns typed
`Vec<DejavueEvent>` records (RFC 3339 timestamp + event discriminator
+ per-event-type fields). `CrushIndex::annotation_history(name)`
returns the chronologically-ordered chain of decision events whose
`decision_title` equals `name`. `codebase.annotation_history` host cap
exposes this to Crush programs.

## Solution

Six files changed. Detail:

### 1. New `crates/crush-index/src/dejavue.rs` (~190 LOC)

Typed `DejavueEvent` struct with serde-compatible flat
`#[serde(default)]` Option fields. Two-stage construction: raw struct
via `serde_json::from_str(line)` → `into_typed` parses the RFC 3339
timestamp via `DateTime::parse_from_rfc3339`, returning `None` on
malformed timestamps (the silent-skip policy matches existing
`load_dejavue()` empty-line skip).

`pub fn parse_timeline_str(content: &str) -> (Vec<DejavueEvent>, usize)`
returns `(events, skipped_count)`. Skipped count is malformed-JSON
OR unparseable-timestamp — empty lines don't count.

`pub fn build_annotation_links(events: &[DejavueEvent]) ->
HashMap<String, Vec<usize>>` is the join layer: strict equality on
`decision_title` for `event == "decision"` events only. Indexes are
positions into the input slice so they're stable across the index's
lifetime.

### 2. Edits to `crates/crush-index/src/index.rs`

- New import: `use crate::dejavue::{build_annotation_links, parse_timeline_str, DejavueEvent};`
- New fields on `CrushIndex`: `dejavue_events: Vec<DejavueEvent>`,
  `annotation_event_links: HashMap<String, Vec<usize>>`
- `Self::new()` initializes the new fields to empty
- `load_dejavue()` now populates BOTH the legacy raw `dejavue_timeline`
  buffer AND the new typed storage + link layer (one read of the file,
  two output buffers)
- New method `set_dejavue_events(events: Vec<DejavueEvent>)` for
  programmatic injection (test fixtures, hosts that generate events)
- New accessor `dejavue_events() -> &[DejavueEvent]`
- New query method `annotation_history(annotation_name) ->
  Vec<&DejavueEvent>` (sorted by `ts` ascending — explicitly re-sorted
  on read so insertion order doesn't matter)

### 3. Edits to `crates/crush-index/src/lib.rs`

Re-export `pub mod dejavue;` + `pub use dejavue::DejavueEvent;` so
downstream crates can use the typed event surface without depending
on the internal `manifest::` module.

### 4. New cap in `crates/crush-lang-sdk/src/codebase.rs`

`CodebaseAnnotationHistoryCap` implements `HostCap` with:
- `name: "codebase.annotation_history"`, `argc: Some(1)`, `returns: true`
- Builds incremental entry maps (each event row carries `ts` always;
  only `branch`/`commit`/`agent`/`decision_title`/`decision_reason`/
  `summary` are surfaced when populated, to keep the wire shape
  compact — no spurious `null` fields)
- Registered in `register_at` after `CodebaseStaleTemporariesCap`
  (order doesn't matter, but placement groups new caps together)

### 5. New `crates/crush-index/tests/dejavue_e2e.rs` (7 tests)

- `parse_timeline_str_drops_malformed_lines_silently`
- `parse_timeline_str_skips_events_with_unparseable_timestamps`
- `build_annotation_links_routes_decision_events_to_matching_title`
- `build_annotation_links_skips_non_decision_events`
- `annotation_history_returns_chronologically_ordered_decisions`
- `annotation_history_returns_empty_vec_for_unknown_name`
- `annotation_history_filters_non_decision_events_via_link_layer`

All 7 pass. `cargo test -p crush-index` total: 32 passing (25 baseline
+ 7 new).

### 6. New integration test in `crates/crush-lang-sdk/tests/codebase_caps_integration.rs`

`integration_annotation_history_cap_via_runtime`: builds a shared
`CrushIndex` from a Crush source string with `@invariant
"use-workspace-deps"` + a timeline NDJSON string with TWO decision
events + ONE file_changed event. Uses the manual caps-build pattern
from `codebase_stale_e2e.rs` (because `Runtime::with_codebase_at`
doesn't accept dejavue yet — see Followups). Asserts the cap returns:
- Both decision events surface (in chronological order, decoupled from
  corpus insertion order)
- The file_changed event does NOT surface (CRUSH-31 linking is strict
  equality on `decision_title` AND `event == "decision"` discriminator)

## Scope Notes

This commit ships the **in-memory** implementation of CRUSH-31. The
ticket spec mentions "extend crush-index SQLite schema" — that
migration is mechanically straightforward (Vec → TABLE, link HashMap
→ JOIN) but a separate ticket distinct from this. The full ticket
spec text is preserved here for traceability and to short-circuit
context-loss when the SQLite migration ticket lands later.

The test scaffolding inherits the existing `codebase_stale_e2e.rs`
manual-caps-build pattern because `Runtime::with_codebase_at` is
scope-bound to source-only ingestion. A `Runtime::with_dejavue(events)
-> Self` builder would let tests share state cleanly AND be useful for
production hosts injecting events programmatically — filed as
followup (S1 below).

## Success Criteria (per ticket spec)

| Item | Status | Evidence |
|------|--------|----------|
| Ingestion of dejavue data alongside CAST | ✅ DONE | `crush-index::parse_timeline_str` parses NDJSON silently; `CrushIndex::load_dejavue` populates both raw + typed storages |
| Querying: `codebase.annotation_history` returns ordered chain | ✅ DONE | cap maps to `idx.annotation_history(name)` which sorts by `ts` ascending |
| M9 STDLIB mock-marker discipline | 🟡 DEFERRED to M9 | The capability underlying M9's gate is live (annotation_history); the M9 fixture itself lands with M9's "103 caps restore + 46 cap rewrites" tracking tickets |
| Round-trip integration test | ✅ DONE | `integration_annotation_history_cap_via_runtime` in `codebase_caps_integration.rs` |
| Annotation history test | ✅ DONE | 7 unit tests in `dejavue_e2e.rs` |
| M9 fixture test | 🟡 DEFERRED | Will land with M9's STDLIB restoration arc (`CRUSH-56`/`CRUSH-57` track these) |

## Non-Goals

- **SQLite persistence**: deferred to a separate migration ticket.
  Structurally compatible (Vec → TABLE, HashMap → JOIN), but out of
  scope for the in-memory layer shipped here.
- **Cross-module dedup**: each event indexed once. If a future
  timeline normalization pass lowercases decision titles or aliases
  them, that's a followup layer before this one, not a rewrite.
- **Git-blame linking**: per the thinker's design analysis, decision
  events currently match only via `decision_title` equality. A future
  ticket could add `event.commit == annotation.last_modified_commit`
  linking once the index tracks last-modified-commit-hash per
  annotation. Not in this turn (no infrastructure to track that
  commit hash today).
- **`codebase.dejavue_events()` cap**: not added this turn — the
  underlying typed events are reachable via `idx.dejavue_events()`
  but a host cap would need a separate design pass for output shape
  (which fields to emit? time range filters?).

## Followups (S1 / S2 from prior review)

- **S1**: `CrushIndex::load_dejavue()` swallows the parser's
  `skipped_count` from `parse_timeline_str`. A production host that
  wants to detect "are we silently dropping malformed timeline lines
  today?" has nothing to read. Recommended: add `pub fn
  load_dejavue_verbose(&mut self) -> usize_skipped` as a sibling method
  (no signature break for existing callers). Narrow, ship-anytime.
- **S2**: `set_dejavue_events([])` leaves `dejavue_timeline` (raw
  buffer) populated if a prior `load_dejavue()` ran. Asymmetry
  surprises callers. Doc-comment is honest about this; recommend a
  one-line clarification that callers using `set_dejavue_events` who
  also want raw-buffer alignment should call `load_dejavue` first.
- **S3 (minor)**: `Runtime::with_dejavue(events) -> Self` builder, as
  described above. Lets the integration test share state cleanly and
  gives production hosts a clean injection point.
- **CRUSH-31-SQLITE-MIGRATE** (ticket for followup work): the SQLite
  schema migration for `dejavue_events` + `annotation_event_links`
  tables. Will mechanically translate the in-memory structures without
  changing the public API of `CrushIndex` or `codebase.rs`.

## Implemented-By Note

This ticket was filed AFTER the implementation landed (the git
commit is the source of truth). The ticket captures what was built —
test counts, success criteria, file inventory — rather than promising
ahead of work. Status set to `Done` from `Backlog` directly to match
the convention from CRUSH-27/28/29 closures.
