# CRUSH-369 — Annotation warnings in `crushc` and `crush-run`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-369 |
| **Priority** | P2 |
| **Status** | In review |
| **Assignee** | nova |
| **Dependencies** | CRUSH-27, CRUSH-30 (W-WIP / W-TMP) |

## Problem

`crush_frontend::check_source` (stale `@wip` W-WIP-001, expired `@temporary`
W-TMP-001, wildcard-in-exhaustive E-EXH-001, `@must-call-after` E-MUT-00x) ran
only in `crush-lint`. `crushc` and `crush-run` — the tools people actually build
and run with — never said anything, so an annotation nobody lints is inert.

## Decision

- One list of passes: `crush_frontend::check_program(&Program)`; `check_source`
  (and so `crush-lint`) calls it.
- `crushc` (every emit except `--emit ast`) and `crush-run run FILE.crush` print
  the findings to stderr as **warnings**, text or `--message-format json`
  (crush-diagnostics schema: `code, level, file, line, col, message, hint`).
  The record level is always `warning`, whatever the pass's own severity: it is
  advisory outside `crush-lint`.
- Never fails the build — except `crushc --deny-warnings`, which prints them and
  exits 1 without writing the artifact. (`crush-run` has no such flag: it runs
  programs, it does not gate them.)

## Success criteria

- [x] `crushc` / `crush build` print W-WIP-001 / W-TMP-001 (text + json).
- [x] `crush-run run` prints them on stderr; program stdout untouched.
- [x] `crushc --deny-warnings` fails the build; clean programs unaffected.
- [x] Tests: `crates/crush-lang-sdk/tests/annotation_warnings_cli.rs`.
