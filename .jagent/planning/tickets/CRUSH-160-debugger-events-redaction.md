# CRUSH-160 — Debugger: event sink, redacted value views, cap-gated debug scopes

| Field | Value |
|-------|-------|
| **ID** | CRUSH-160 |
| **Priority** | P3 |
| **Status** | Done (PR pending merge) |
| **Phase** | M3 |
| **Assignee** | nimbus-c |
| **Dependencies** | CRUSH-159 |
| **Estimated effort** | M (~50 turns) |
| **Relay lane** | C2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

A debugger embedded in a host (vortex, crush-web, an IDE) needs events, not a REPL; and a capsule debugger must not leak values the host didn't grant. nanovm and vm-runtime have an event-sink trait, `DebugVisibility{None,ControlOnly,InspectRedacted,Full}` and redacted `ValueView` (type + content hash).

## Success criteria

- [x] `DebugEventSink` trait (stop reason, frame snapshot) with a channel and a collecting impl
- [x] visibility levels; `InspectRedacted` shows type + hash, never content
- [x] debug operations gated by named caps (`debug.inspect`, `debug.step`) through `HostCaps`
- [x] tests for each visibility level

## Technical approach

- Build on CRUSH-159's stop machinery.

## Files to modify

- `crates/crush-debugger/src/`
- `crates/crush-vm/src/portable_vm.rs` (snapshot accessors)

## Non-goals

- encrypted-execution debugging

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/debug/types.rs`, `crates/platform/sdk/vm-runtime/src/vm_debug/snapshots.rs`

## Outcome (2026-10-07, nimbus-c)

Stacked on CRUSH-159 (crush-ast#103) → CRUSH-176 (#101).

- **Grants** (crush-vm `debug`): `debug.step` (control), `debug.inspect.redacted`
  (values as type + keyed hash), `debug.inspect` (values in full); the last two need
  `debug.step`. `HostCaps::grant_debug(DebugVisibility)` registers presence-only gates
  (same pattern as `polyglot.*`); a program that declares and calls one gets an error.
  `DebugVisibility::granted(Option<&HostCaps>)` → `None | ControlOnly | InspectRedacted | Full`.
- **Redaction** (crush-vm): `Redactor` (per-session `RandomState` key) renders `Value` →
  `ValueView::{Hidden, Redacted{type_name, hash}, Plain{type_name, text}}`;
  `PortableVm::frame_snapshot(depth, &Redactor) -> FrameSnapshot{depth, function, ip, locals}`,
  `function_at(ip)`, `debug_visibility()`. A snapshot never holds a raw value.
- **Events + gating** (crush-debugger): `events::{DebugEvent, StopReason, DebugEventSink,
  CollectingSink}`, `mpsc::Sender<DebugEvent>` is a sink; events are `Send` and carry only
  `ValueView`s. `DebugSession` reads its level from the driver's grants once at `new`,
  refuses commands the grants don't allow (message + `Refused` event naming the grant),
  renders every value through the redactor, emits `Output`/`Stopped{reason, frames}`/
  `Finished`/`QuotaExceeded`/`Error`; `handle_command` is now `pub` for embedding hosts.
  New `VmDriver` methods (`debug_visibility`, `frames`, `take_output`) have defaults —
  `debug_visibility` defaults to `None`, so a driver must opt in.
- **CLI**: `--cap debug.*` become debugger grants (kept out of the program's permissions);
  unknown `debug.*` names and `debug.inspect*` without `debug.step` are errors. **Behaviour
  change:** with no debug grant the REPL now refuses to run the program — the ticket's
  "debug operations gated by named caps"; the existing CLI tests pass the full grants.
- The REPL now prints the program's own output (TASKS gap found in CRUSH-159).

**Where the gate sits:** in `DebugSession`, the boundary a debug client talks to.
`PortableVm`'s step/watch/breakpoint methods stay ungated host APIs (exo-light uses
`set_breakpoints` for output capture; gating them would break it).

## Evidence

- crush-vm `debug::tests`: grants → visibility (incl. inspect-without-step = None), each
  level's view, per-session hash key (equal within, different across, `2` ≠ `2.0`),
  `frame_snapshot` (function names, parked IP, redacted locals), VM visibility from host
  caps, **refusal**: a program calling `debug.step` errors with or without the grant.
- crush-debugger `session` unit tests: every control command refused without `debug.step`
  (nothing reaches the driver, one `Refused` event each), ControlOnly refuses `print`,
  inspect levels allow it.
- `crush-debugger/tests/events_test.rs` (real `PortableVmDriver`): Full — watch stop with
  frames, step into `inc` (frames innermost first), `Output("11\n")`, `Finished`;
  InspectRedacted — no `Plain` value anywhere; ControlOnly — all `Hidden`; None — only
  `Refused`; channel sink across threads.
- `repl_integration_test.rs` via the binary: no grant → 4 refusals and the program never
  runs; `debug.step` → `<hidden>`; `debug.inspect.redacted` → `<int #…>`; bad grants exit 1.
- `cargo test -p crush-vm -p crush-debugger -p crush-lang-sdk`: 583 passed, 0 failed.
  `cargo check -p crush-vm --no-default-features --target wasm32-unknown-unknown`: clean.
- Live run of the binary at all four levels (transcript excerpt in the README).
