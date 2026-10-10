# CRUSH-249 — `crush-walk-run` exits 0 when the program fails

| Field | Value |
|-------|-------|
| **ID** | CRUSH-249 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M11 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 CASM↔WASM test drive |

## Problem

When the walked program fails at runtime (`unknown capability: func_1`, `load from
uninitialised slot 0`, quota exceeded), `crush-walk-run` prints `Error: …` and exits 0, so
scripts and CI can't tell a failed run from a good one.

## Success criteria

- [ ] Runtime errors exit non-zero (1), like `crush-run`; `sys.exit(n)` exits `n`.
