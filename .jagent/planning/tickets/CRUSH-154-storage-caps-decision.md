# CRUSH-154 — `storage.*` handle-based store capabilities — port or decline

| Field | Value |
|-------|-------|
| **ID** | CRUSH-154 |
| **Priority** | P4 |
| **Status** | Closed — declined (2026-10-07) |
| **Phase** | M9 |
| **Assignee** | nimbus (lane A5, derby phase 2) |
| **Dependencies** | decision C-6 |
| **Estimated effort** | S (~40 turns) |
| **Relay lane** | A5 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

exo's `storage.open/read/write/size/close` is a handle-based store over the HAL. crush-ast has `db.query/execute` (`db` feature, rusqlite) and no `storage.*`. Osmosis's scalar-only `HostValue` doesn't fit a handle API either.

## Success criteria

- [x] Decision C-6 recorded via `dejavue decision`
- [ ] ~~If **port**~~ (not taken): caps behind a new `--store <dir>` grant, handles scoped to the VM, closed on VM drop, tests for use-after-close
- [x] If **decline**: a note in `docs/` mapping `storage.*` use cases to `db.*`/`fs.*`, and this ticket closed

## Technical approach

- Default recommendation: decline unless a consumer appears.

## Files to modify

- `crates/crush-lang-sdk/src/host_caps.rs` (if ported)

## Non-goals

- a key-value database engine

## Source (reference only — re-implement, don't copy)

- exo `crates/core/base/stdlib/src/storage.rs` (263 L)

## Resolution (2026-10-07, nimbus — lane A5)

**Declined** per decision C-6 (captain, s474): no `storage.*`; `db.*` covers persistence.

- [`docs/design/storage-caps-declined.md`](../../../docs/design/storage-caps-declined.md) maps each
  `storage.*` use to `db.*` / `fs.*` and records what a future port would need.
- Recorded with `dejavue decision`. No code change.
- Checked: no `storage.*` caller in `examples/`, `crates/`, `crush-language-guide` or
  `crush-notebook`. The doc's key-value recipe was run live (`crush-run --db`, `INSERT OR REPLACE`
  twice then `SELECT` → `[{v: hi}]`).
