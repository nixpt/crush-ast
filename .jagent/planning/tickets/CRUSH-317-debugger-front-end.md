# CRUSH-317 — Make `crush-debugger` usable: `crush debug FILE`, `crush-debug`, NDJSON mode

| Field | Value |
|-------|-------|
| **ID** | CRUSH-317 |
| **Priority** | P2 |
| **Status** | In review (PR open) |
| **Phase** | M3 |
| **Assignee** | nova |
| **Dependencies** | CRUSH-159, CRUSH-160 (merged) |
| **Estimated effort** | M |

## Problem

`crush-debugger` is a 2,100-line library with a REPL, breakpoints, step/next/finish,
watchpoints and grant-gated inspection (CRUSH-159/160), but no front end compiled
Crush source: its own `crush-debugger run` binary assembles CASM text only, the umbrella
`crush` had no `debug` subcommand, `crush-installer` installed neither, and the
compiler's source map was the *generated assembly's* line numbers, so `break file:line`
could not mean a line the author wrote. Locals were bare slot numbers.

## Success criteria

- [x] `crush debug FILE` and a `crush-debug` binary compile `.crush` through `crush run`'s pipeline
      and start the REPL; the program's own capabilities take `crush run`'s flags.
- [x] Debug grants are explicit flags (`--step`, `--inspect`, `--inspect=redacted`), default none;
      a refused command names the flag.
- [x] `--message-format json`: `DebugEvent`s as NDJSON, errors in `crush-diagnostics` records.
- [x] Verified on closures, `impl` methods, loops, calls (transcript in the PR).
- [x] `crush-installer` installs the new binary; `crush --help` lists `debug`.
- [x] crush-debugger README documents it.

## Outcome

- **Source map in source lines** (`crush-lang-sdk` `casm_to_vm`): each CASM instruction already
  carried its line in `meta`; the map now goes offset → Crush line (instructions without a line
  drop out). `compile_crush_source_for_debug` also returns `LocalNames` (function → name/slot).
- **`crush-debugger`**: names for `print`/`watch` (`print p.x`), `locals`, `stepi`/`nexti`, line-level
  `step`/`next` (opt-in via `set_line_stepping`), `break LINE`, one breakpoint per function a line
  has code in (a one-line closure), stops report `file:line in function` plus the source line,
  `ndjson` module + `run_ndjson`, pluggable grant hint.
- **Dependency**: `crush-lang-sdk` -> `crush-debugger` (optional `debug` feature, on by default;
  crush-web builds with defaults off). crush-debugger depends on crush-vm and crush-diagnostics only.
- **Fixes**: `Redactor` hashing overflowed the stack on a closure that captures itself;
  `BreakpointSet::remove` used the deprecated `IndexMap::remove`.

## Not done / follow-ups

- crush-run and crush-debug each build their host caps from the same flags; the flag struct should
  move into `crush_lang_sdk::cli` (not done here so crush-run's file stays untouched).
- `gpu.launch` is not wired into `crush-debug`; `.cvm1` blobs have no source map and are refused.
- The debugger drives `PortableVm` directly, not the scheduler `crush run` uses; a program that
  uses `spawn`/`await` has not been tried and may not behave as under `crush run`.
