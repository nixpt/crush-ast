# CRUSH-378 — Compiler: map `& | ^ << >> ~` to the existing VM opcodes

| Field | Value |
|-------|-------|
| **ID** | CRUSH-378 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

Bitwise operators are refused although the VM has the opcodes.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

`print(6 & 3)` reports `Unknown operator: &` on `crush-run`. Re-run independently on `495d9995` through `python_walker` then `crush-run run`.

## Success criteria

- [ ] All six operators compile and match CPython on non-negative integers
- [ ] Negative and overflow behaviour follows the CRUSH-384 decision
