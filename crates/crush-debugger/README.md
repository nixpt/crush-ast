# crush-debugger

Interactive runtime debugger for Crush packages.

## Status: SCAFFOLD (initial commit)

This crate ships a coherent surface (modules, public API, parsers,
breakpoint registry, VM driver trait, in-process session) but
**does not yet pause on a real breakpoint**. Every component that
needs the upstream `crush_vm::PortableVm` BP-pause hook (latent at
`portable_vm.rs:1037`) is wired behind `todo!()` macros so the
integration seam is *loud* during code review, not silent.

## What's real (and unit-tested)

| Module | Surface | Tests |
|--------|---------|-------|
| `wire_consumer` | `parse_record`, `consume_stream`, `OwnedDiagRecord`, `ParseRecordError` | 5 |
| `breakpoint`    | `BreakpointSet`, `Breakpoint`, `BreakpointId`, `Location` | 4 |
| `repl`          | `parse_command`, `Command`, `ParseCommandError`             | 10 |
| `vm_driver`     | `VmDriver`, `PortableVmDriver`, `StepOutcome`, `VmState`, `VmRunResult`, `VmError` | 4 |
| `session`       | `DebugSession`, `MockVmDriver` (test)                        | 4 |

`wire_consumer::parse_record` round-trips a hand-authored
`DiagRecord` against `crush_diagnostics::diag_line` so the parser
matches the canonical emitter byte-for-byte (mirrors the lockdown
test in `crush_pkg::main::handle_lint_with_byte_exact_three_rule_fedpath`).

`breakpoint::BreakpointSet` is keyed on `<file>:<line>` with
monotonic IDs and a `BTreeMap` for stable insertion-order iteration.

`repl::parse_command` accepts long verbs (`break`, `step`, `continue`,
`list`, `print`, `delete`, `quit`, `help`) plus single-letter aliases
(`b`, `s`, `c`, `l`, `p`, `d`, `q`, `h`, `?`).

## What's NOT real (next-iteration blockers)

1. **Real BP pause.** `PortableVmDriver::run_until_breakpoint_or_done`
   uses a heuristic step-loop with a hard cap until the upstream hook
   at `crush_vm::portable_vm.rs:1037` lands.
2. **`file:line` -> bytecode coord.** `Breakpoint.bytecode_address`
   stays `None` until `crush_frontend` ships a sourcemap.
3. **REPL eval.** `DebugSession::run_repl` is `todo!()` because binding
   `Command -> VmDriver` actions needs (1).
4. **APIs (TUI / DAP / VS Code extension).** Only the `clap` CLI shell
   exists today. The REPL stdin loop and any DAP/LSP adapter are
   placeholders wired at the trait seam.

## CLI

```text
$ crush-debugger version
$ crush-debugger run <TARGET> [--strict]
$ crush-debugger repl
```

`version` confirms the scaffold is loaded; `run` and `repl` print
the upstream blocker they are gated on (no business logic yet).

## Workspace layout

```toml
# Cargo.toml (path deps):
crush-vm          = { workspace = true }
crush-diagnostics = { workspace = true }
```

External deps are all `workspace = true` (clap, anyhow, serde,
serde_json). No new external crates added by this scaffold.

## Bumping to a real debugger

When the upstream `crush_vm::PortableVm` BP-pause hook lands:

1. Replace `PortableVmDriver::run_until_breakpoint_or_done` body with
   a `step()` loop that inspects the new `VmYield::BreakpointHit`
   variant and returns `VmRunResult::HitBreakpoint`.
2. Populate `Breakpoint.bytecode_address` from a `crush_frontend`
   sourcemap (or instruct the driver to look it up at BP-time).
3. Implement `DebugSession::run_repl` as a `parse_command -> match
   on Command -> drive the VmDriver` loop.
4. Flip the `run_repl_panics_with_todo_macro_until_upstream_hook_lands`
   test to a positive test that exercises a full break/step/continue
   cycle.
