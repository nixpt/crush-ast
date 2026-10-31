# CRUSH-316 — crush-lint reports false syntax errors, misses real ones, and `--json` prints text

| Field | Value |
|-------|-------|
| **ID** | CRUSH-316 |
| **Priority** | P1 |
| **Status** | Done (pending review) |
| **Assignee** | nova |
| **Filed by** | nova — 2026-10-30, branch `agent/nova/CRUSH-316` |

## Problem

On main `dcd5c823` (crush-lint 0.8.0), with `let a = 1\nprint(undefined_name)\n`:

1. `crush-lint` printed `error[E-SYNTAX]: Missing semicolon` for line 1, which is valid
   Crush (no semicolons). The real problem (`undefined_name`) was not reported.
2. `--json` printed human text; `--message-format json` took `json` as the file name.

## Root cause

`main.rs` did not call the parser at all. It scanned lines: any line containing `let `
without a `;` was "Missing semicolon", any line containing `def ` was "Unexpected token".
`AiLinter::augment_diagnostic` then wrote an invented "forgot a semicolon" hint. On
`examples/crush/` that produced an `error` on 22 of 51 files, 13 of the 41 that run.

## Change

- Findings come from `crush-frontend`: parser errors (`E-SYNTAX`, with line/col),
  `check_source` passes (`E-EXH-001`, `W-WIP-001`, `E-MUT-*`, ...), and the semantic
  checker (`E-SEMA`; it stops at the first error and has no position).
- Hints are advice on the `hint` field of a diagnostic, never a diagnostic. The
  semicolon / `def` / generic-refactor heuristics and the unrelated `decisions.md`
  snippet are gone.
- `--json` and all three `--message-format` forms emit `crush_diagnostics` NDJSON on
  stdout, nothing else.

## Success criteria

- No `error` on any `examples/crush/*.crush` that runs; the ten `// expect-error:` files
  produce the runner's own error. (`crates/crush-lint/tests/lint.rs`)
