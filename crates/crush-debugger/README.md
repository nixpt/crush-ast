# crush-debugger

Interactive runtime debugger for Crush programs. It drives
`crush_vm::PortableVm` and pauses on the VM's own hooks: breakpoints,
steps and watchpoints all surface as `VmYield::DebugBreak`, and
`PortableVm::last_stop()` says which one fired.

## Grants

Debugging a program is a capability like any other: nothing is ambient.
The session reads its level once, from `debug.*` grants in the VM's
`HostCaps` (`HostCaps::grant_debug(level)`; the CLI takes them as `--cap`):

| Grants | Level | Allows |
|--------|-------|--------|
| none | `None` | `help`, `list`, `status`, `quit` only; everything else is refused, naming the missing grant |
| `debug.step` | `ControlOnly` | breakpoints, steps, `continue`, watchpoints; every value shows as `<hidden>` |
| `+ debug.inspect.redacted` | `InspectRedacted` | `print`, values as `<type #hash>`: equal values hash equal within one session, the key changes per session |
| `+ debug.inspect` | `Full` | values in full |

`debug.inspect*` without `debug.step` grants nothing. The `debug.*` gates
are for the debugger: a program that declares and calls one gets an error.
The program's own output is not inspection and always shows.

## Commands

| Command | What it does |
|---------|--------------|
| `break <file>:<line>` / `b` | breakpoint, resolved to a bytecode offset through the assembler source map |
| `delete <id>` / `d` | remove a breakpoint |
| `step` / `s` | execute one instruction (enters calls) |
| `next` / `n` | step over: one instruction, running any call it makes to completion |
| `finish` / `fin` | step out: run until the current function returns (from the entry function: to the end) |
| `continue` / `c` | run until a breakpoint, a watchpoint, a quota, or the end |
| `watch <slot> [top \| frame <depth>]` / `w` | stop after an instruction changes a local slot; default scope is the current frame |
| `unwatch <id>` | remove a watchpoint |
| `print <slot>` / `p` | value of a local slot in the current frame |
| `list` / `l`, `status` / `i`, `help` / `h`, `quit` / `q` | |

Stepping and watching work at bytecode level, by call depth:

- a step never stops where it started; `next` stops at the next
  instruction no deeper than where it began, `finish` at the next one
  shallower. A breakpoint or watchpoint hit on the way ends the step.
- a breakpoint and a finished step pause *before* their instruction; a
  watchpoint pauses *after* the instruction that changed the slot. A
  change includes the slot's first assignment and in-place edits of an
  array or map it holds.
- `frame <depth>` watches one activation (1 = entry frame); `top` watches
  whichever function is running and re-reads the slot silently on calls
  and returns.

Locals are numbered slots: compiled bytecode keeps no variable names, and
only the assembler emits a line map. Watching by name and line-level
stepping need a source map from `crush-frontend`.

The same hooks are public on `PortableVm` (`request_step`, `cancel_step`,
`add_watchpoint`, `remove_watchpoint`, `call_depth`, `local`,
`last_stop`, and `frame_snapshot` / `debug_visibility` for redacted views),
so any host that steps the VM, such as crush-web's `Session`, can use
them without this crate. Those are host APIs; the grant check sits in
`DebugSession`, the boundary a debug client talks to.

## Events

A host that embeds the debugger instead of using the REPL drives
`DebugSession::handle_command` and attaches a sink:

```rust
let (tx, rx) = std::sync::mpsc::channel();
session.set_event_sink(tx);            // or a CollectingSink
session.handle_command(parse_command("continue")?)?;
for event in rx.try_iter() { /* DebugEvent */ }
```

Events: `Stopped { reason, frames }` (reason: breakpoint, step, watchpoint
with old/new, paused; frames innermost first, each with function name, IP
and locals), `Output`, `Finished`, `QuotaExceeded`, `Error`, `Refused`.
Every value in an event is a `ValueView` rendered at the session's level,
never a raw VM value, and events are `Send`.

## CLI

```text
$ crush-debugger run <FILE> [--cap NAME]... [--break FILE:LINE]...
                 [--max-steps N] [--max-stack N] [--max-output N] [--max-call-depth N]
$ crush-debugger version
```

`run` loads a CASM text file (`crush_vm::assemble`), not Crush source.

```text
$ crush-debugger run tests/fixtures/calls.crush --cap io.print \
    --cap debug.step --cap debug.inspect
cru-s-debugger> watch 0
watchpoint #0 set on slot 0 in frame 1
cru-s-debugger> continue
watchpoint #0: slot 0 (depth 1) (unset) -> 5
cru-s-debugger> print 0
slot 0 = 5
cru-s-debugger> next
stopped at ip 21 (depth 1)
cru-s-debugger> next
stopped at ip 24 (depth 1)
```

(The second `next` ran the whole `CALL inc`.) With `--cap
debug.inspect.redacted` instead of `debug.inspect`, the same session shows
`(unset) -> <int #f1f2057bd1863c82>` and `slot 0 = <int #f1f2057bd1863c82>`.

## Library

`DebugSession` owns a `VmDriver` (today `PortableVmDriver`), the
breakpoint registry and the source map. `VmDriver`'s step/watch/locals
methods have defaults, so drivers written before them still compile; they
report "not supported".
