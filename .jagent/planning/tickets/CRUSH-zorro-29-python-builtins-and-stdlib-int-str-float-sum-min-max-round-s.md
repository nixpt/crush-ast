# CRUSH-zorro-29 — Python: builtins and stdlib — int/str/float/sum/min/max/round/sorted/list/any/all/isinstance/type, math, __name__

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-29 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 (closures) for `sorted(key=)` / `map` / `filter` |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Common builtins fail at run time with `unknown capability: <name>` (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.):
`int("42")`, `float`, `int(3.9)`, `bool`, `sum`, `min`, `max`, `round`, `sorted`, `list(range(..))`,
`list(reversed(..))`, `any`, `all`. Also:
- `isinstance(3, int)`, `type(x).__name__` → `load from uninitialised slot 0`.
- `if __name__ == "__main__": main()` → `load from uninitialised slot 0` (`__name__` undefined) — this
  guard is in most real scripts.
- `import math` → `unknown capability: module.load` (CRUSH-200 lists `math.sqrt`).

## Fix

- Lower each builtin in the Python walker to an existing VM op/cap where one exists (`len`, `str`,
  arrays) or to a `py.*` cap implemented once in crush-vm (numeric conversions with Python's rules:
  `int("x")` → ValueError, `round` = banker's rounding, `min`/`max`/`sum` over any iterable).
- `__name__` is `"__main__"` for the entry module.
- `import math` / `from math import ...`: map the module to existing `math.*` caps.
- `sorted(key=)`, `map`, `filter` once function values / closures land (CRUSH-251).

## Done when

- [ ] Every builtin above matches python3 in a cpython_corpus program; `__name__` guard runs `main()`.
