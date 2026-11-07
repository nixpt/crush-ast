# CRUSH-379 — Python walker: map `min`, `max`, `abs`, `str`, `int` onto existing capabilities

| Field | Value |
|-------|-------|
| **ID** | CRUSH-379 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

Capabilities exist under other names, but the Python call doesn't reach them.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

`print(max(1, 2))` reports `Undefined function: max` on `crush-run`. Re-run independently on `495d9995` through `python_walker` then `crush-run run`. The audit also found `math.sin` and `math.cos` in the catalog but failing from Python, cause untraced.

## Success criteria

- [ ] Each builtin runs and matches CPython on a test set
