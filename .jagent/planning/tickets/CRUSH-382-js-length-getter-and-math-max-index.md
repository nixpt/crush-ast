# CRUSH-382 — JS: `.length` must honour a class getter; `Math.max` results usable as an index

| Field | Value |
|-------|-------|
| **ID** | CRUSH-382 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-277 |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

`.length` bypasses `get length()`, and `Math.max` returns a float that can't be used as an array index. Both stop FixturePlayer at runtime with its import stubbed.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

From the audit; not independently re-run when filed.

## Success criteria

- [ ] A class getter named `length` is called
- [ ] An integer-valued `Math.max` result indexes an array
