# CRUSH-164 — Wildcard + expiry in `Quotas::allowed_caps` (optional)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-164 |
| **Priority** | P5 |
| **Status** | Done |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`Quotas::allowed_caps` is an exact-name allowlist. exosphere's (unwired) enforcer supported `fs.*`-style scopes and expiry. Only needed if a consumer (exo-light, osmosis) wants pattern grants.

## Success criteria

- [x] `fs.*` matches `fs.read` but not `fsx.read`; `*` alone requires an explicit opt-in
- [x] optional expiry instant per entry
- [x] tests for match/non-match/expired

## Technical approach

- Small matcher in crush-vm caps; no change to the exact-name fast path.

## Files to modify

- `crates/crush-vm/src/caps.rs` / `portable_vm.rs::dispatch_cap`

## Non-goals

- scoped resources (`fs.read:/dir`) — capsule-contract's job

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/sdk/vm-runtime/src/enforcement.rs::Scope::matches`

## Resolution

`Quotas::cap_allowed` (crush-vm `vm.rs`) is the single matcher. `allowed_caps` stays `Option<Vec<String>>` (plain string = exact name). `prefix.*` entries match children only (not `fsx.*`, not the bare prefix). A bare `*` is inert unless `Quotas::allow_any_cap` is set. Per-entry expiry via `Quotas::allowed_caps_expiry` / `allow_cap_until`. Routed sites: `scheduler::check_cap_permitted` (covers every CAP_CALL/EXEC_LANG in both engines), `PortableVm::awaits_io_read`, `Runtime::missing_grants`. Tests: `crates/crush-vm/src/tests/cap_patterns.rs`.
