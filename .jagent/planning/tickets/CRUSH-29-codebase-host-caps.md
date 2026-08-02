# CRUSH-29 — `codebase.*` host capabilities over `crush-index`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-29 |
| **Priority** | P1 — depends on CRUSH-28 (index data layer); unlocks the navigation layer for `crush-notebook` AI cells |
| **Status** | Done |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-28 (data layer) — must precede this ticket so caps have something to query |
| **Estimated effort** | M |

> Status refreshed s412 per triage: e4302ad

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section —
implements `ai-native-roadmap.md` Steps 5-6: "codebase.* cap
implementations" and "host cap provider wired into default SDK runtime."

## Problem

`ai-native-roadmap.md` describes the agent session protocol (verbatim):

```crush
let map = codebase.modules();
let inv = codebase.invariants("scheduler");
let sites = codebase.exhaustive_sites("Value");
let gaps = codebase.uncovered_paths();
let callers = codebase.callers("execute_one");
```

**None of these caps exist today.** Even after CRUSH-28 builds
`crush-index`, there's no `HostCap` trait implementation that surfaces
the index to a running Crush program. Without this, the M5 band is
structurally complete but functionally invisible — agents can't query
the index without writing Rust against `crush-index` directly. AI
writers (the project's stated primary user per `PROJECT.md`)
need `codebase.*` as host caps, not as Rust APIs.

## Success criteria

- [ ] `codebase.modules | invariants | uncovered_paths | exhaustive_sites
      | callers` implemented as `HostCap` trait impls in
      `crush-lang-sdk` (5 caps in this ticket; `semantic_search`
      deferred — see Non-goals).
- [ ] Caps registered in the default `crush-lang-sdk::runtime`
      provider, called from `crush-repl`, `crush-run`, and
      `crushc --run` (parity with how `io.*` and `fs.*` are wired).
- [ ] **6 integration tests**, one per cap: e.g.
      `crush --eval "io.print(codebase.modules().len())"` passes for a
      sample repo, `codebase.uncovered_paths()` returns at least one
      entry against `crush-vm`'s known-uncovered `VmError::*` paths.
- [ ] `codebase.*` caps honor the same permission mediation rules as
      existing `io.*` and `fs.*` caps — no path traversal, no
      arbitrary filesystem reads beyond what the cap declares.
- [ ] No additional `codebase.*` cap (semantic_search) is filed in
      this ticket; that work is a follow-up CRUSH-NN post-M5.

## Technical approach

1. **Trait impls.** New `crates/crush-lang-sdk/src/codebase_caps.rs`
   module with one `HostCap` impl per cap name (5 in this ticket).
   Each impl accepts a path argument (default to current workspace)
   and delegates to `crush-index`'s query layer (CRUSH-28).
2. **Provider registration.** Extend `crush-lang-sdk/src/host_caps.rs`
   to register the 5 new providers in the default provider set, with
   the same permission patterns as `io.*`.
3. **Tests.** 6 integration tests via `crushc --emit json` + a small
   harness that calls each cap and asserts the response shape.

## Files to modify

- `crates/crush-lang-sdk/src/host_caps.rs` — register the 5 new providers
- `crates/crush-lang-sdk/src/codebase_caps.rs` (new) — 5 HostCap impls
- `crates/crush-lang-sdk/tests/crush_test.rs` — integration tests

## Non-goals

- **`codebase.semantic_search`.** Requires an embedding model + index
  query that hovers near AI infrastructure not part of M5. Defer to a
  separate CRUSH-NN ticket post-M5.
- **Cross-workspace queries.** v0 only queries the local workspace's
  index. Aggregation across multiple indexes is a separate concern.
- **Capability mediation rewrite.** Existing `HostCap` permission
  patterns are reused; this ticket doesn't redesign the cap
  mediation layer (M7's CRUSH-43 firewall is downstream of this).

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 3 of 8
- `docs/design/ai-native-roadmap.md` Steps 5-6 — the canonical spec
- CRUSH-28 (depends on — needs the index)
- CRUSH-32 (parallel — VM-side AI-opcode consumers of these caps)
- CRUSH-31 (parallel — extends `codebase.*` with `annotation_history`)
- M5 thesis example in `ai-native-roadmap.md` Section "Agent Session
  Protocol"
