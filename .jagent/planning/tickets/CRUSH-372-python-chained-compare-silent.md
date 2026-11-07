# CRUSH-372 — Python walker: chained comparisons keep only the first pair

| Field | Value |
|-------|-------|
| **ID** | CRUSH-372 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | Maya audit |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | altair — 2026-11-06, from satya's Maya-through-Crush audit |

## Problem

`0 <= x < 3` evaluates only `0 <= x`, with no error and on both compile doors. In the audited corpus that is 59 sites in 18 files, including core simulation code. A wrong bounds check here is silent, so this is the highest-priority item from the audit.

Measured at `main` `495d9995` (v0.11.1) by the 2026-11-06 Maya-through-Crush audit.

## Evidence

```python
x = 5
print(0 <= x < 3)        # CPython: False, Crush: True
if 0 <= x < 3: print(1)   # CPython: skips, Crush: prints 1
```
Re-run independently on `495d9995` through `python_walker` then `crush-run run`.

## Success criteria

- [ ] `a op b op c` lowers to `a op b and b op c` with `b` evaluated once, or is refused loudly
- [ ] Both doors agree with CPython, including when `b` has side effects
