# CRUSH-384 — Design: deterministic RNG and integer semantics for Maya on Crush

| Field | Value |
|-------|-------|
| **ID** | CRUSH-384 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

A real Python tick needs decisions, not only more lowering. Maya seeds CPython's `random.Random(seed)`, including with string seeds, while Crush has SplitMix64. Python integers are unbounded and the VM's overflow, and `worldgen._hash` depends on that.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

Also needed: the Python object model on COSM (classes, dataclasses, tuple keys), how Python modules load, and which compile door is canonical. Hand-ported simulation code matched real Maya bit for bit (390/390 lines over 1000 ticks), so the arithmetic and control flow are sound. From the audit; not independently re-run when filed.

## Success criteria

- [ ] A recorded decision for RNG, integers, object model, modules and canonical door
- [ ] Follow-up tickets filed from it
