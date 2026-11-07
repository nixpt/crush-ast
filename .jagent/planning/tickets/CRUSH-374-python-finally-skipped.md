# CRUSH-374 — Python: `finally` skipped on `return`, `break` and raising handlers

| Field | Value |
|-------|-------|
| **ID** | CRUSH-374 |
| **Priority** | P1 |
| **Status** | Done — merged in PR #408 (`f65db104`, 2026-11-07); see Resolution |
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

- [x] `finally` runs on every exit path, or the construct is refused loudly
- [x] Tests cover `return`, `break`, `continue` and a raising handler

## Resolution

PR #408 (altair) lowers `try A except B finally C` to `try { try { A } catch { B } } catch (e) { C; throw e } C`, so `C` runs exactly once, outside the protection of `B`. A `try` with no `finally` is unchanged.

- **Early exits are refused, not implemented, as the criteria allow:** a `return`, `break` or `continue` that leaves the `try`, including a `return` inside `finally`, is refused with a clear error. Jumps that stay inside, and a `return` in a nested `def`, are still allowed. In a copy of the Maya source, 1 of 20 `try/finally` blocks hits this.
- **Also fixed:** a `finally` that raised could be caught by its own `except` and then run twice. That case wasn't in the audit.
- **Tests:** corpus program `26_try_finally` (CPython-checked) covers a raising handler. `try_finally_test.rs` covers the refusals for `return`, `break`, `continue`, a nested `return` and a `return` in `finally`.
- **AOT:** a `finally` stays VM-only, because the AOT backends refuse exceptions. `26_try_finally` is on the loud list as CRUSH-zorro-7.
- **JS:** CRUSH-199 still covers `finally` in JavaScript.
