# CRUSH-221 — Float literals with exponents (`1.5e10`, `1e-7`) don't parse

| Field | Value |
|-------|-------|
| **ID** | CRUSH-221 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`print(1.5e10)` and `print(1e-7)` are parse errors in every engine. Other languages'
walkers can produce these values, and AOT C/`%g` prints them in this form.

## Success criteria

- [ ] Lexer accepts `[0-9]+(\.[0-9]+)?[eE][+-]?[0-9]+`; tests; tree-sitter grammar updated (CRUSH-207).
