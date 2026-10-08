# CRUSH-156 — `ai_native.*` caps take real arguments

| Field | Value |
|-------|-------|
| **ID** | CRUSH-156 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M (~40 turns) |
| **Relay lane** | B1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

All 10 `ai_native.*` caps declare `argc: Some(0)` and drop VM-stack arguments (`crates/crush-lang-sdk/src/ai_native.rs:87-104`); FastVM's `resolve_host_request` discards `HostRequest{args}`. No real AI backend can be plugged in until arguments arrive. This is CRUSH-55's PORT #1 prerequisite.

## Success criteria

- [ ] each `ai_native.<kind>` spec declares its real argc (or variadic) and receives the compiled arguments on CVM1, PortableVm and FastVM
- [ ] the CAST argument shape for tool lists follows nanovm's `parse_tools`
- [ ] existing echo behaviour stays the default backend, now echoing the received args
- [ ] differential test: same AI program, same echoed args on all three engines

## Technical approach

- Compiler side already lowers the AI expressions; fix the cap specs and the dispatch paths.
- ⚠ `crates/crush-vm/src/fastvm/` is lane-guarded — get foreman's ack before editing `resolve_host_request`.

## Files to modify

- `crates/crush-lang-sdk/src/ai_native.rs`
- `crates/crush-vm/src/{scheduler,portable_vm}.rs` (AI opcode dispatch)
- `crates/crush-vm/src/fastvm/mod.rs` (⚠ lane guard)

## Non-goals

- any real model backend

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/vm/ai.rs::parse_tools`; CRUSH-55 inventory App. C §2a
