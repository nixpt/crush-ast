# CRUSH-160 — Debugger: event sink, redacted value views, cap-gated debug scopes

| Field | Value |
|-------|-------|
| **ID** | CRUSH-160 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M3 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-159 |
| **Estimated effort** | M (~50 turns) |
| **Relay lane** | C2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

A debugger embedded in a host (vortex, crush-web, an IDE) needs events, not a REPL; and a capsule debugger must not leak values the host didn't grant. nanovm and vm-runtime have an event-sink trait, `DebugVisibility{None,ControlOnly,InspectRedacted,Full}` and redacted `ValueView` (type + content hash).

## Success criteria

- [ ] `DebugEventSink` trait (stop reason, frame snapshot) with a channel and a collecting impl
- [ ] visibility levels; `InspectRedacted` shows type + hash, never content
- [ ] debug operations gated by named caps (`debug.inspect`, `debug.step`) through `HostCaps`
- [ ] tests for each visibility level

## Technical approach

- Build on CRUSH-159's stop machinery.

## Files to modify

- `crates/crush-debugger/src/`
- `crates/crush-vm/src/portable_vm.rs` (snapshot accessors)

## Non-goals

- encrypted-execution debugging

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/debug/types.rs`, `crates/platform/sdk/vm-runtime/src/vm_debug/snapshots.rs`
