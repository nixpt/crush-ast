# CRUSH-zorro-34 — VM runtime errors as catchable typed exceptions (ZeroDivisionError, IndexError, KeyError, ...)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-34 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | VM |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Errors raised by the VM itself can't be caught by language-level `try`: `try: 1/0 except
ZeroDivisionError: print("div0")` stops the program with `Error: division by zero` (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.:
`exceptions`; also listed in CRUSH-200). The same holds for index/key errors and JS `try/catch` of
runtime TypeErrors. Only values thrown with `THROW` are catchable.

## Fix

When an instruction fails with a catchable `VmError` (DivByZero, ArrayBounds, missing map key,
TypeError, NotCallable/FuncArity, ...) and a try handler is active in the current task, convert it to a
thrown error value carrying a **kind** (`ZeroDivisionError`, `IndexError`, `KeyError`, `TypeError`)
and message, and unwind like `THROW`. Quotas and `Exit` stay uncatchable. Walkers map
`except ZeroDivisionError` / JS `catch` onto the kind. Both engines (scheduler + portable VM).

## Done when

- [ ] `1/0`, `[1][5]`, `{}["k"]` are catchable by type in Python and by `catch` in JS, on both engines;
  uncaught they still exit non-zero with today's message.
