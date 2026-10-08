# CRUSH-158 — `QueryProvider` / `DelegationBackend` traits + delegation selection

| Field | Value |
|-------|-------|
| **ID** | CRUSH-158 |
| **Priority** | P3 |
| **Status** | Done (PR pending review) |
| **Phase** | M5 |
| **Assignee** | nimbus-b |
| **Dependencies** | CRUSH-156 |
| **Estimated effort** | S (~30 turns) |
| **Relay lane** | B3 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`ai_native.query` and `ai_native.agent_delegation` have no seam for a host to supply a real backend. exosphere's delegation has reusable selection logic (first_available / broadcast / best / round_robin, result-format validation) buried under hard-coded fleet paths.

## Success criteria

- [x] `QueryProvider` and `DelegationBackend` traits; `HostCapsBuilder` accepts implementations; default = current echo
- [x] selection strategies + format validation ported as pure code with tests
- [x] no filesystem polling, no hard-coded paths

## Technical approach

- Traits live in `crush-lang-sdk`; the antarikshya ai-core host implements them.

## Files to modify

- `crates/crush-lang-sdk/src/ai_native.rs`
- `crates/crush-lang-sdk/src/host_caps.rs`

## Non-goals

- any concrete backend

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/runtimes/ai/src/{query,delegation}.rs`

## Evidence (nimbus-b, 2026-10-07)

- `crates/crush-lang-sdk/src/ai_native/providers.rs`: `QueryProvider::query(&QueryRequest) ->
  Result<Json, String>`; `DelegationBackend::{status(agent) -> Option<AgentStatus{available,
  rating}>, dispatch(agent, task) -> Result<String, String>}`; `QueryCap` / `DelegationCap`;
  pure `select()` and `Format::{parse, validate}`. Re-implemented from the inventory summary —
  the exosphere source polls status files under a fixed directory and dispatches to fleet
  tooling; here everything the selection knows comes from the backend.
- Wiring: `HostCapsBuilder::query_provider` / `delegation_backend` (+ `ai_native::register_with`).
  Only with `ai_native(true)`; the toolchain snapshot is taken after, so tool steps reach them.
- Choices: nobody available → empty selection, `ok: false`, nothing dispatched (exosphere
  fell back to the first agent regardless of status); `capability_match` / `parallel_split` /
  `hierarchical` / `consensus` and unknown formats are errors, not silent fallbacks.
- Tests: 7 unit tests (each strategy, ties, edge cases, format validation, dispatch +
  validation statuses, query request plumbing) + `tests/ai_providers.rs` (compiled CAST
  program reaches both backends; backends without the grant → gates absent, never called;
  echo stubs when no backend; a toolchain step reaches the provider).
