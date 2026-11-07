# CRUSH-380 — Python: let functions read module globals

| Field | Value |
|-------|-------|
| **ID** | CRUSH-380 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

A function can't read a module-level name. The audit counted about 1040 reads across 64 files.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

```python
K = 3
def f():
    return K
print(f())   # crush-run: Undefined variable: K
```
Re-run independently on `495d9995` through `python_walker` then `crush-run run`.

## Success criteria

- [ ] Module globals are readable inside functions on both doors
