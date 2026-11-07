# CRUSH-374 — Python: `finally` skipped on `return`, `break` and raising handlers

| Field | Value |
|-------|-------|
| **ID** | CRUSH-374 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-199 |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

A `finally` body doesn't run when the `try` exits through `return` or `break`, or through a raising handler, and nothing reports it. The audit counted 5 Maya sites. CRUSH-199 covers the JS side only. This item was in the audit's findings but not its suggested ticket list.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

```python
def f():
    try:
        return 1
    finally:
        print(9)
print(f())   # CPython: 9 then 1, Crush: 1
```
The same happens with `break` in a loop. Re-run independently on `495d9995` through `python_walker` then `crush-run run`.

## Success criteria

- [ ] `finally` runs on every exit path, or the construct is refused loudly
- [ ] Tests cover `return`, `break`, `continue` and a raising handler
