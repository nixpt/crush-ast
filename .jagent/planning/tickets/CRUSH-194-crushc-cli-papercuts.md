# CRUSH-194 — `crushc` CLI papercuts: `--check` misses codegen errors, `--emit types` = `--emit ast`, `--invariant-runtime` unrunnable

| Field | Value |
|-------|-------|
| **ID** | CRUSH-194 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- `crushc -C` on a file containing only `break` says "no errors detected"; a full
  compile fails with `break outside of loop` (`--check` stops before `Compiler::compile`).
- `--emit types` is byte-identical to `--emit ast` (both call `render_program`). The
  renderer also drops `-> Int`, prints `@arr_set(a, 0, 5)`, `P {}` and
  `# NOTE: SetField is not parseable from text` — output isn't valid Crush.
- `--invariant-runtime` emits `CAP_CALL "invariant.evaluate" 0` (no args; check
  source not passed) and no runtime registers that cap — programs built with it can
  never run, even with `--cap` (`compiler.rs:~160-190`).
- `crushc --emit casm -o -` writes a file literally named `-` instead of stdout.
- `--emit casm` output lacks a trailing newline.
- Error caret is misaligned when the source line contains a tab.

## Success criteria

- [ ] Each bullet fixed or the flag removed/documented.
