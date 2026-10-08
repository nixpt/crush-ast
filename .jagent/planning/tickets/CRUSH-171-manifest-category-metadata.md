# CRUSH-171 — Package manifest category / platform metadata

| Field | Value |
|-------|-------|
| **ID** | CRUSH-171 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~25 turns) |
| **Relay lane** | D4 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`crush-pkg`'s `Manifest` has no category or target-platform fields; crush-capsules' `CAPSULE_CATEGORIES.md` / `hub.json` sketched a scheme for cataloguing capsules.

## Success criteria

- [ ] optional `category` and `platforms` fields in the manifest (serde-default, old manifests still load)
- [ ] `crush-pkg show` prints them; validation of known values with a clear error
- [ ] schema documented

## Technical approach

- Additive manifest change.

## Files to modify

- `crates/crush-pkg/src/manifest.rs`
- docs

## Non-goals

- a registry / hub service

## Source (reference only — re-implement, don't copy)

- `nixpt/crush-capsules` `CAPSULE_CATEGORIES.md`, `hub.json`
