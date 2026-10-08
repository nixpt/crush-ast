# CRUSH-161 — Make `crush-pkg` publishable (unblocks squeeze)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-161 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M0 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~20 turns) |
| **Relay lane** | D1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`crush-pkg` is the only crate squeeze needs that is not on crates.io (checked 2026-10-07; its deps are all published at 0.3.9, `crush-buckets` 0.1.0). It is simply absent from CRUSH-104's publish set.

## Success criteria

- [ ] `cargo publish --dry-run -p crush-pkg` passes (readme path, `buckets` dep carries `version`, every internal dep has `path` + `version`)
- [ ] `crush-pkg` added to the publish lane / `crates-publish-sync` order
- [ ] the actual `cargo publish` is left to foreman (irreversible) — this ticket ends at a green dry-run + a DM

## Technical approach

- Fix whatever the dry-run reports; no API changes.

## Files to modify

- `crates/crush-pkg/Cargo.toml`
- publish-lane config

## Non-goals

- publishing (foreman's action)
- renaming the crate

## Source (reference only — re-implement, don't copy)

- `nixpt/squeeze` RELEASE.md / STATE.md
