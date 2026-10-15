# CRUSH-zorro-27 — crush-run runs walker-language files (.py, .js, .rs, ...) directly

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-27 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) / CLI |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-249 (walk-run exit status), CRUSH-184 (output lost on runtime error) |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Only `crush-walk-run` (a crush-aot dev binary) and `crush-aotc` accept walker languages. The user-facing
`crush run` / `crush-run` reject them: `unsupported file extension: py (expected .crush, .casm, or .cvm1)`
(`crates/crush-lang-sdk/src/bin/crush-run.rs:254`, `:469`). So "run example.py with crush" needs a dev tool,
which also registers no stdlib caps (CRUSH-204) and exits 0 on runtime errors (CRUSH-249).

## Fix

`crush-run` dispatches walker extensions (`.py`, `.js`/`.mjs`, `.rs`, `.c`, `.go`, `.zig`, `.sh`, ...)
through the walker registry → CAST → CASM → CVM1, with the same stdlib caps, quotas and exit codes as
`.crush`. `crush-walk-run` becomes a thin alias or is retired.

## Done when

- [ ] `crush run example.py` runs it; a runtime error exits non-zero and keeps earlier output.
- [ ] Same for `.js` and `.rs`; an unknown extension still errors.
