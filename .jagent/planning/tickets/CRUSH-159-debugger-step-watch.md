# CRUSH-159 — Debugger: step over/out and watchpoints over `PortableVm`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-159 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M3 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M (~70 turns) |
| **Relay lane** | C1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`crush-debugger` has bytecode breakpoints and a REPL (`crates/crush-vm/src/portable_vm.rs:121-279`) but no step over/out and no watchpoints. nanovm's debugger has both and is used live by vortex's shell, which blocks vortex from ever moving off nanovm. Its README/`lib.rs:27` still describe `todo!()` hook points that no longer exist.

## Success criteria

- [ ] `step into / over / out` by frame depth on `PortableVm` (pause-before-instruction, like CRUSH-118's io.read pause)
- [ ] watchpoints on locals/globals (break when value changes), with a scope
- [ ] behaviour tests that run real programs and assert where execution stops — not typecheck stubs
- [ ] README and `lib.rs` docs match the code

## Technical approach

- Re-implement the concepts against `PortableVm`; nanovm's code is coupled to its `Task`/`VmContext`.
- Keep any new hook usable by `crush-web`'s `Session`.

## Files to modify

- `crates/crush-vm/src/portable_vm.rs`
- `crates/crush-debugger/src/{lib,session,repl}.rs`
- `crates/crush-debugger/README.md`

## Non-goals

- source-line breakpoints (need the frontend source map — M3's other item)

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/debug/` (3,166 L; `StepMode`, `WatchScope`)
