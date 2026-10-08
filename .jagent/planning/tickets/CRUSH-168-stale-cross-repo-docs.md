# CRUSH-168 — Fix stale in-code docs pointing at exosphere/ecasm

| Field | Value |
|-------|-------|
| **ID** | CRUSH-168 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M0 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | E1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

Several crush-ast docs still describe things that moved or were deleted: `crates/casm/src/lib.rs` mentions `ecasm.rs` (removed by CRUSH-80); `crates/crush-cast/STATUS.md` points at exosphere paths (`../base/errors`, `crates/core/base/stdlib/`) that don't exist here; `crates/crush-lang-sdk/src/stdlib.rs` says stdcaps are "always available" (they need the `stdlib` feature — CRUSH-113).

## Success criteria

- [ ] each stale reference fixed or removed
- [ ] `crush-cast/STATUS.md` names crush-ast's own stdlib (`crush-lang-sdk/src/stdlib*`) and links MIGRATION-INVENTORY
- [ ] no code changes

## Technical approach

- Docs-only sweep.

## Files to modify

- `crates/casm/src/lib.rs` (comment)
- `crates/crush-cast/STATUS.md`
- `crates/crush-lang-sdk/src/stdlib.rs` (header, if CRUSH-113 hasn't already)

## Non-goals

- rewriting the guide

## Source (reference only — re-implement, don't copy)

- MIGRATION-INVENTORY §1–2
