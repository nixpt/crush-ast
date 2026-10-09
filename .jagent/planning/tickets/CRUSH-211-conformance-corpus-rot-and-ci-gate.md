# CRUSH-211 — Conformance corpus fails 19/37 annotated files; not in CI

| Field | Value |
|-------|-------|
| **ID** | CRUSH-211 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-123 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`conformance` exits 1: **18 pass, 19 fail, 12 skipped**. It isn't run in CI, so
the corpus rotted unnoticed.

Annotation drift (16 of 19 — partly tracked at `TASKS.md:~349`):
- `// expect-error:` headers say `[runtime] …`; the runner now prints
  `compile error:` / `runtime error:`.
- Line numbers are off by one: the annotation line was added at the top after the
  expected text was captured.
- Headers are truncated with `...`.

Real issues:
- `fibonacci.crush` fails with `instruction quota exceeded (1000)`: `run_crush`
  hard-codes a 1K-step budget while its doc comment says 50k (program needs 1948 steps).
- `build_pipeline.crush` expects `call depth quota exceeded (256)` but now succeeds — a
  bug got fixed; the annotation should flip to `expect-exit: 0` with expected output.
- `greeting.crush` / `sysinfo.crush` now fail at compile time (`Undefined function: str`)
  instead of the annotated runtime error.
- `conformance --help` ignores the flag and runs the corpus.

## Success criteria

- [ ] Runner matches on the error *message*, independent of the stage prefix, or the
      annotations state the stage (`expect-compile-error:` / `expect-runtime-error:`).
- [ ] Step budget matches the doc (or is per-file overridable).
- [ ] All annotations refreshed by hand (each reviewed, not blindly regenerated).
- [ ] CI job runs `conformance` and fails on any regression.
