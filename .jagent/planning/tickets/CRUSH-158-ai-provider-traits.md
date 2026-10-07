# CRUSH-158 — `QueryProvider` / `DelegationBackend` traits + delegation selection

| Field | Value |
|-------|-------|
| **ID** | CRUSH-158 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-156 |
| **Estimated effort** | S (~30 turns) |
| **Relay lane** | B3 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`ai_native.query` and `ai_native.agent_delegation` have no seam for a host to supply a real backend. exosphere's delegation has reusable selection logic (first_available / broadcast / best / round_robin, result-format validation) buried under hard-coded fleet paths.

## Success criteria

- [ ] `QueryProvider` and `DelegationBackend` traits; `HostCapsBuilder` accepts implementations; default = current echo
- [ ] selection strategies + format validation ported as pure code with tests
- [ ] no filesystem polling, no hard-coded paths

## Technical approach

- Traits live in `crush-lang-sdk`; the antarikshya ai-core host implements them.

## Files to modify

- `crates/crush-lang-sdk/src/ai_native.rs`
- `crates/crush-lang-sdk/src/host_caps.rs`

## Non-goals

- any concrete backend

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/runtimes/ai/src/{query,delegation}.rs`
