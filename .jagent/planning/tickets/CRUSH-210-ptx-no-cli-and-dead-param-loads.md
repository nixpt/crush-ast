# CRUSH-210 — crush-ptx: no CLI path to emit PTX; each kernel param loaded twice

| Field | Value |
|-------|-------|
| **ID** | CRUSH-210 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M11 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- No user-facing way to emit PTX: `crush-aotc` doesn't depend on `crush-ptx`
  (TASKS mentions a "crush-aotc PTX backend scaffold").
- Each kernel param is `ld.param`'d twice; the first loads (`%r0`/`%r1` in
  `test_compile_basic_ptx`) are dead.

## Success criteria

- [ ] `crush-aotc --emit ptx` (or similar) wired up, with a golden-output test.
- [ ] One `ld.param` per parameter.
