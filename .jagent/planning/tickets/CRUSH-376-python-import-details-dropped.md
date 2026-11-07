# CRUSH-376 — Python walker: keep all import names, aliases and relative levels

| Field | Value |
|-------|-------|
| **ID** | CRUSH-376 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-380 |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

`import a, b` keeps only `a`, `as` aliases are dropped (6 sites) and relative levels are dropped (228 sites). The import refusal hides this today, so it becomes a silent bug as soon as module loading works.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

From the audit; not independently re-run when filed.

## Success criteria

- [ ] Every name, alias and relative level survives into CAST
- [ ] A test pins each form
