# CRUSH-159 — Debugger: step over/out and watchpoints over `PortableVm`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-159 |
| **Priority** | P2 |
| **Status** | Done (PR pending merge) |
| **Phase** | M3 |
| **Assignee** | nimbus-c |
| **Dependencies** | none |
| **Estimated effort** | M (~70 turns) |
| **Relay lane** | C1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`crush-debugger` has bytecode breakpoints and a REPL (`crates/crush-vm/src/portable_vm.rs:121-279`) but no step over/out and no watchpoints. nanovm's debugger has both and is used live by vortex's shell, which blocks vortex from ever moving off nanovm. Its README/`lib.rs:27` still describe `todo!()` hook points that no longer exist.

## Success criteria

- [x] `step into / over / out` by frame depth on `PortableVm` (pause-before-instruction, like CRUSH-118's io.read pause)
- [x] watchpoints on locals (break when value changes), with a scope — CVM1 has no globals; see Outcome
- [x] behaviour tests that run real programs and assert where execution stops — not typecheck stubs
- [x] README and `lib.rs` docs match the code

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

## Outcome (2026-10-07, nimbus-c)

Stacked on CRUSH-176 (crush-ast#101), which fixed the PortableVm IP bug that
made stepping recursive programs unreliable.

- **crush-vm** `src/debug.rs` + `PortableVm` (additive API): `request_step(StepMode::{Into,Over,Out})`,
  `cancel_step`, `add_watchpoint(slot, WatchScope::{Frame(depth),Top}) -> WatchId`,
  `remove_watchpoint`, `watchpoints`, `call_depth`, `local(depth, slot)`, `last_stop() -> DebugStop`.
  Every stop is still `VmYield::DebugBreak`, so existing hosts (crush-web `Session`) need no change.
  A step never stops where it started; Over stops at depth ≤ start, Out at depth < start. A
  breakpoint/watch hit ends a pending step. Watch compares a deep, type-exact snapshot, so in-place
  array/map edits and `2 → 2.0` count as changes; the first assignment counts; a watch pauses
  *after* the changing instruction (breakpoints and steps pause before).
- **crush-debugger**: REPL `next`/`n`, `finish`/`fin`, `watch <slot> [top|frame <d>]`/`w`,
  `unwatch <id>`, `print <slot>` (was NYI); `VmRunResult::{Stepped,Watchpoint}`; new `VmDriver`
  methods have defaults (crush-visuals implements `VmDriver`); README/lib.rs rewritten.

**Scope note — the ticket said "locals/globals":** CVM1 frames have no global store, and compiled
bytecode keeps no slot names (`casm_to_vm` maps names → slots and drops the map). Watchpoints are
therefore by slot, like breakpoints are by offset; names and line stepping wait for a frontend
source map (the M3 item already listed under Non-goals).

## Evidence

- `crush-vm` `debug::tests` (12 behaviour tests over assembled programs: into/over/out, over a
  RET, out of entry, breakpoint-wins, watch Frame/Top, in-place array edit, watch ends step,
  remove) — `cargo test -p crush-vm --lib`: 168 passed.
- `crush-lang-sdk/tests/debugger_step_watch.rs`: compiled Crush source — a watch on `sum_to`'s
  accumulator sees `0, 1, 3, 6, 10`; step-out lands in `main` before `print`; stepping over
  `main` never enters `sum_to`, stepping into does.
- `crush-debugger/tests/repl_integration_test.rs`: three REPL runs of the real binary
  (`watch`+`continue`+`print`, `next` ×5 with exact IPs over a CALL, `finish` from inside a call).
- `cargo test -p crush-vm -p crush-debugger -p crush-lang-sdk`: 562 passed, 0 failed.
- `cargo check -p crush-vm --no-default-features --target wasm32-unknown-unknown`: clean.
- Live run of `crush-debugger run tests/fixtures/calls.crush` — transcript in the README.
- Found on the way (captured in TASKS): the debugger never shows the program's own output.
- Commit: `c2f470f`; PR crush-ast#103 (stacked on #101).
