# CRUSH-zorro-32 — Python: functions — default/keyword args, *args/**kwargs, global/nonlocal, generators

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-32 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 (closures: lambda, nested def, nonlocal) |
| **Estimated effort** | L |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Function features beyond plain positional params (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.):
- `def g(n, greet="hi")` → `default argument values are not yet supported` (CRUSH-200: was a stack
  underflow); keyword arguments `g("al", greet="yo")`.
- `def f(*args, **kw)` → runtime `load from uninitialised slot 0`.
- `global count` → `global keyword not yet supported`.
- generators (`yield`) and generator expressions (`sum(x for x in xs)`) → not supported; also blocks
  `docs/benchmarks/sieve.py`.
- `lambda`, nested-def closures, `nonlocal` → CRUSH-251 steps 3–4 (in progress).

## Fix

- Defaults / keywords: the walker resolves a call against the callee's signature when it is known
  statically (fill defaults, reorder keywords); calls through function values need a calling
  convention with names (decide with CRUSH-251).
- `*args` / `**kw`: pack extra positional args into a list / keyword args into a dict at the call site.
- `global`: module-level variables as VM globals (needs a global slot space, shared with `__name__`).
- Generators: a design — green tasks + channels, or a CPS/state-machine lowering; decide first.

## Done when

- [ ] Probes `defaults`, `varargs`, `globalkw`, `generator` match python3 (generators may be split out).
