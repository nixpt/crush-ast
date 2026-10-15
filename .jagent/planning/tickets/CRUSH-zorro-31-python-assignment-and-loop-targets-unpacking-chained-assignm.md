# CRUSH-zorro-31 — Python: assignment and loop targets — unpacking, chained assignment, enumerate/zip targets, walrus

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-31 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-2 (tuple values), CRUSH-200 (lists unpacking) |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Assignment / loop target forms beyond a plain name fail (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.):
- `a, b = 1, 2`, `a, b = b, a`, `x, y = point` → `unsupported assignment target: Tuple(...)`.
- `a = b = 4` → `multi-target assignment not yet supported`.
- `for i, (a, b) in enumerate(zip(...))` → `for loop target must be a name`; same for
  `for k, v in d.items()`.
- `if (n := 5) > 3:` → `__crush_assign__ target must be a variable`.

Tuple *values* (printing `(1, 2)`) are CRUSH-zorro-2; this ticket is the binding side.

## Fix

Destructure through a temp: evaluate the right side once into `__tmp`, then assign `__tmp[0]`,
`__tmp[1]`, ... (recursively for nested targets; starred `a, *rest = xs` via slice). Loop targets use
the same lowering on each element. Walrus → assignment expression (store + load). Chained assignment →
one evaluation, several stores. `enumerate`/`zip` as `py.*` caps or walker desugaring.

## Done when

- [ ] Probes `tuples`, `swap`, `multiassign`, `enumzip`, `walrus` match python3; a length mismatch is a
  ValueError, not silent.
