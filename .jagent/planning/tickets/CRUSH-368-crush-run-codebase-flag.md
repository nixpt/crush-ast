# CRUSH-368 — `crush-run --codebase` / `crush-repl --codebase`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-368 |
| **Priority** | P2 |
| **Status** | In review |
| **Assignee** | nova |
| **Dependencies** | CRUSH-31 (the `codebase.*` caps) |

## Problem

The eleven read-only `codebase.*` capabilities (modules, definition, callers,
invariants, exhaustive_sites, wip, temporaries, decisions, ...) existed only for
Rust embedders (`Runtime::with_codebase_files`). No binary could hand a program
an index, so an agent could not ask Crush about Crush source from Crush.

## Decision

The flag is the grant: `--codebase FILE_OR_DIR` (repeatable) indexes those
files — a directory expands to every `*.crush` below it, sorted — and registers
the caps over them. Without it the caps are absent, so `crush-run` refuses the
program up front (`--codebase FILE: codebase.callers, ...`) and
`crush-run caps` names the flag. Module name = file stem; two files with the
same stem are an error, not a silent overwrite.

## Success criteria

- [x] `crush-run run PROG --codebase PATH...` serves `codebase.*` over the given files/dirs.
- [x] `crush-repl --codebase PATH...` likewise.
- [x] Missing flag: refusal names `--codebase FILE`; `crush-run caps` lists the grant.
- [x] Bad path / unparseable file / duplicate module name are loud errors.
- [x] Test: `crates/crush-lang-sdk/tests/codebase_flag.rs` (two files + a querying program; dir expansion; refusal; repl).
