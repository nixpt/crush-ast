# CRUSH-383 — swc: keep re-exported names

| Field | Value |
|-------|-------|
| **ID** | CRUSH-383 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-340 |
| **Estimated effort** | S |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

swc barrels keep only `Import` statements, so re-exported names vanish. `index.js` and `exports.js` compile to 2 instructions. The import refusal hides this today.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

From the audit; not independently re-run when filed.

## Success criteria

- [ ] `export … from …` names survive on swc, or the construct is refused loudly
