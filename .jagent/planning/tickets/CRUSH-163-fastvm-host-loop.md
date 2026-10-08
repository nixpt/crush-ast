# CRUSH-163 — FastVM yield-servicing host loop (decision-gated, lane-guarded)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-163 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | decision C-3 |
| **Estimated effort** | M (~70 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`run_fastvm_with_caps` runs once with `DummyHal` and returns the first yield (`crates/crush-vm/src/vm.rs:774-794`); nothing services `CallHost`/`ExecLang`/`Spawn`/`Await`, and `watchdog`/`restart` lower but are never serviced. nanovm's VM is a full FastVM host driver. Also: nanovm spells three AI ops `ai_goal_decl`/`ai_knowledge_share`/`ai_tool_chain` (ast: `ai_goal_declaration`/`ai_knowledge_sharing`/`ai_toolchain`). Only matters if FastVM stays a sanctioned engine (CRUSH-55 D-4).

## Success criteria

- [ ] decision C-3 recorded
- [ ] if go: a host loop servicing every yield kind through `HostCaps`, parity with CVM1 under the differential harness (CRUSH-77)
- [ ] nanovm AI spellings accepted as lowerer aliases
- [ ] watchdog/restart either serviced with tests or rejected at lowering with a clear error

## Technical approach

- ⚠ `crates/crush-vm/src/fastvm/` is lane-guarded — foreman ack required.

## Files to modify

- `crates/crush-vm/src/vm.rs`
- `crates/crush-vm/src/fastvm/`

## Non-goals

- JIT changes

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/vm/mod.rs:932-1080`; nanovm `tests/supervision_tests.rs` (ancestor)
