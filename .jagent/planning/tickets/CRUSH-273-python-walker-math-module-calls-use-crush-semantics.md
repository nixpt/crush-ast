# CRUSH-273 — The Python walker passes `math.*` calls through with Crush semantics

| Field | Value |
|-------|-------|
| **ID** | CRUSH-273 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | — |
| **Dependencies** | Related: CRUSH-zorro-29 (Python builtins and stdlib), CRUSH-200 |
| **Estimated effort** | S–M |
| **Filed by** | claude — 2026-10-16, found while building CRUSH-272 |

## Problem

`crush-lang-python` lowers a call `m.f(args)` on a module name in its `MODULES` list
(`math`, `json`, `random`, …) to a Crush call named `m.f` — the Crush stdlib capability of
that name, with Crush's semantics. Where those differ from CPython the result is silently
different:

- `math.floor(2.5)` / `math.ceil(2.1)` printed `2.0` / `3.0`; CPython prints `2` / `3`.
  CRUSH-272 makes both a walk-time error until they lower to an exact int.
- Not yet audited: domain errors (`math.sqrt(-1)`, `math.log(0)` raise `ValueError` in
  CPython), and every other `math.*` / `json.*` / `random.*` the stdlib happens to have.
  Calls with no stdlib capability (`math.trunc`, `json.dumps`) fail loudly (CRUSH-272
  catches them at compile time for lowered blocks).

## Reproduction

```python
import math
print(math.floor(2.5))   # CPython: 2 — walker (before CRUSH-272): 2.0
print(math.sqrt(-1))     # CPython: ValueError — walker: check
```

## Technical approach

An allowlist of module calls whose Crush capability matches CPython exactly (with a
CPython-parity test per entry in `cpython_corpus/`); everything else a walk-time error.
`math.floor` / `math.ceil` / `math.trunc` lower to an exact int (a loud error for
inf / NaN, as CPython raises).
