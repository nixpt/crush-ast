# CRUSH-381 — boa: exported functions can read module consts

| Field | Value |
|-------|-------|
| **ID** | CRUSH-381 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-276 |
| **Estimated effort** | S |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

`export const K=3; export function f(){return K}` fails, though the non-exported form works. This was the cause of 3 first blockers in maya-engine.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

Through default `js_walker` then `crush-run run`: `Undefined variable: K`. Re-run independently on `495d9995` through `js_walker` then `crush-run run`. The boa-specific refusal is from the audit.

## Success criteria

- [ ] The exported form compiles and matches node on both lowerings
