# crush-debugger

Interactive runtime debugger for Crush programs. It drives
`crush_vm::PortableVm` and pauses on the VM's own hooks: breakpoints,
steps and watchpoints all surface as `VmYield::DebugBreak`, and
`PortableVm::last_stop()` says which one fired.

## Usage

Debug a `.crush` file with `crush debug` (or its binary, `crush-debug`; both
come with `crush-lang-sdk` and `crush-installer`):

```text
$ crush debug prog.crush --step --inspect
cru-s-debugger> break 18
breakpoint #0 set at prog.crush:18
cru-s-debugger> continue
hit breakpoint #0 at prog.crush:18 in __lambda_0_scale
    18 |         let scaled = n * k
cru-s-debugger> print k
k = 3
cru-s-debugger> watch scaled
cru-s-debugger> continue
watchpoint #0: slot 2 (depth 3) (unset) -> 3 at prog.crush:19 in __lambda_0_scale
cru-s-debugger> finish
```

It compiles the file the way `crush run` does, so `break FILE:LINE` (or just
`break LINE`) means a line you wrote, `print`/`watch` take variable names
(`print p.x` reads a field; `locals` lists the frame), and `step`/`next` run
to the next source line (`stepi`/`nexti` go by instruction). A line that holds
code in two functions (a one-line closure) gets a breakpoint in each. Commands
come from stdin, one per line, so a script can pipe them in; `--break LINE`
sets breakpoints before the first command. Never leave stdin open and unread
(the program's own `io.read` shares it).

Debugging is a capability and nothing is ambient, so **by default it grants
nothing**: only `help`, `list`, `status` and `quit` work, and the program does
not run. Ask for what you need:

| Flag | Grants | Lets you |
|------|--------|----------|
| `--step` | `debug.step` | `break`, `step`, `next`, `finish`, `continue`, `watch`; values show as `<hidden>` |
| `--step --inspect=redacted` | `+ debug.inspect.redacted` | `print`/`locals`, values as `<type #hash>` |
| `--step --inspect` | `+ debug.inspect` | values in full |

A refused command names the flag (`refused: `print` needs the
`debug.inspect.redacted` grant (pass --step --inspect=redacted, or --inspect)`).
The program's own capabilities are granted with `crush run`'s flags (`--fs`,
`--env`, `--time`, `--polyglot`, `--cap NAME`, `--max-steps`, ...); one the
program needs and the run does not grant is refused before anything runs.

### For editors and agents

`--message-format json` drops the prompt and prose and writes one JSON object
per line to stdout: `started`, `stopped` (reason, file, line, source text,
function, every frame with its locals by name), `output`, `finished`,
`quota_exceeded`, `error`, `refused` (with the flag to pass) and `message` (the
answer to `break`, `print`, `list`, ...). A command line that did not parse, or
failed, is a `crush-diagnostics` record (`code` `E-DBG01` / `E-DBG02`). Values
are `{"hidden":true}`, `{"type":"int","hash":"..."}` or `{"type":"int","text":"3"}`
according to the grants; nothing else leaves the session.

```text
$ printf 'break 18\ncontinue\nprint k\nquit\n' | crush debug prog.crush --step --inspect --message-format json
{"event":"started","file":"prog.crush","visibility":"Full"}
{"command":"break 18","event":"message","text":"breakpoint #0 set at prog.crush:18"}
{"event":"stopped","file":"prog.crush","function":"__lambda_0_scale","line":18,"reason":{"id":0,"kind":"breakpoint"},"source":"        let scaled = n * k","frames":[...]}
{"command":"print k","event":"message","text":"k = 3"}
```

`crush-debugger run FILE.casm` (below) is the older entry point for CASM
assembly text; `crush debug` also takes `.casm`, with the assembly's lines
and slot numbers instead of names. A compiled `.cvm1` carries no source map
and is refused.

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
| `break <file>:<line>` / `break <line>` / `b` | breakpoint, resolved to a bytecode offset through the source map (a line with no code is reported, with the next line that has some) |
| `delete <id>` / `d` | remove a breakpoint |
| `step` / `s` | execute one instruction (enters calls); to the next source line under `crush debug` |
| `next` / `n` | step over: one instruction (or source line), running any call it makes to completion |
| `stepi` / `nexti` | `step` / `next` by instruction even when they go by line |
| `finish` / `fin` | step out: run until the current function returns (from the entry function: to the end) |
| `continue` / `c` | run until a breakpoint, a watchpoint, a quota, or the end |
| `watch <slot \| name> [top \| frame <depth>]` / `w` | stop after an instruction changes a local slot; default scope is the current frame |
| `unwatch <id>` | remove a watchpoint |
| `print <slot \| name[.field]>` / `p` | value of a local in the current frame, or a field/element of it |
| `locals` / `vars` | every assigned local of the current frame |
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

Compiled bytecode keeps no variable names and the assembler's line map is the
assembly's own, so at this level locals are numbered slots. `crush debug`
supplies both from the compiler (`crush_lang_sdk::compile::compile_crush_source_for_debug`):
the source map in your lines and the names, which a host passes to
`DebugSession::set_source_name` / `set_local_names` / `set_line_stepping`.

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
