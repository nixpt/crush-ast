# CRUSH-152 — `async.sleep` as an alias of `time.sleep`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-152 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | A3 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`examples/crush/async_test.crush` is `expect-error: unknown capability: async.sleep`. exosphere and the ancestor registered `async.sleep` as a synchronous sleep — identical to crush-ast's `time.sleep` (behind `--time`, honours `max_wall_time_ms`).

## Success criteria

- [ ] `async.sleep` registered behind `--time`, calling the same implementation as `time.sleep` (one shared fn, not a copy)
- [ ] `async_test.crush` passes (or its header documents what else it needs)
- [ ] a test proves `async.sleep` past the wall-clock limit returns `CapTimeout`

## Technical approach

- Register a second `HostCap` name over the shared sleep fn in `host_caps.rs`.

## Files to modify

- `crates/crush-lang-sdk/src/host_caps.rs`
- `examples/crush/async_test.crush`

## Non-goals

- a real async/await sleep that yields to the scheduler (that is SPAWN/YIELD/AWAIT territory)

## Source (reference only — re-implement, don't copy)

- exo `crates/core/base/stdlib/src/async_cap.rs` (62 L)
