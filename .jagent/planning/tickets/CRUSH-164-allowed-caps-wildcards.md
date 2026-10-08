# CRUSH-164 — Wildcard + expiry in `Quotas::allowed_caps` (optional)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-164 |
| **Priority** | P5 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`Quotas::allowed_caps` is an exact-name allowlist. exosphere's (unwired) enforcer supported `fs.*`-style scopes and expiry. Only needed if a consumer (exo-light, osmosis) wants pattern grants.

## Success criteria

- [ ] `fs.*` matches `fs.read` but not `fsx.read`; `*` alone requires an explicit opt-in
- [ ] optional expiry instant per entry
- [ ] tests for match/non-match/expired

## Technical approach

- Small matcher in crush-vm caps; no change to the exact-name fast path.

## Files to modify

- `crates/crush-vm/src/caps.rs` / `portable_vm.rs::dispatch_cap`

## Non-goals

- scoped resources (`fs.read:/dir`) — capsule-contract's job

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/sdk/vm-runtime/src/enforcement.rs::Scope::matches`
