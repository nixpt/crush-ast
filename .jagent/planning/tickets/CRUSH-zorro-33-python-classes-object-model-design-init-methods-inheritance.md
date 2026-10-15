# CRUSH-zorro-33 — Python: classes — object model design (init, methods, inheritance, dunder methods, custom exceptions, with)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-33 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) / VM |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 (closures — methods are functions with `self`) |
| **Estimated effort** | L |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

`class` is not supported at all (`class definitions not yet supported`), which also blocks custom
exceptions (`class E(Exception)`), context managers (`with`), `__str__`, and most real programs
(Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.: `classes`, `inherit`, `dunderstr`, `raisecustom`, `withstmt`).

## Fix — design first

Write a short design (like CRUSH-251's) for an object model shared by Python classes and JS objects /
prototypes (CRUSH-zorro-16):
- instances as maps + a class reference; classes as maps of function values (CRUSH-251 step 2);
- method lookup through the class and its bases (MRO for single inheritance first), `super()`;
- dunder dispatch where the VM already has hooks: `__init__`, `__str__`/`__repr__` (print),
  `__eq__`, `__len__`, `__getitem__`, `__enter__`/`__exit__`;
- exceptions as class instances; `isinstance` against the hierarchy.
Engines: VM first; AOT/FastVM loud.

## Done when

- [ ] Design agreed; probes `classes`, `inherit`, `dunderstr`, `raisecustom`, `withstmt` match python3.
