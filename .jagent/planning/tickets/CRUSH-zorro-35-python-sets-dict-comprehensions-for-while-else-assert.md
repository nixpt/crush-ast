# CRUSH-zorro-35 — Python: sets, dict comprehensions, for/while-else, assert

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-35 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-2 (`in` for set membership) |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Smaller Python statements and literals that fail loudly (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.):
- set literals / `set()` / `.add` → `set literals not yet supported` (the VM has `Value::Set`).
- `{k: v for ...}` → `dict comprehensions not yet supported` (needs a dynamic-key map insert; `d[k] = v`
  works, so the comprehension can lower to a loop with an index-assign).
- `for ... else:` / `while ... else:` → `for ... else is not yet supported`.
- `assert cond, msg` → `assert not yet supported` (→ raise AssertionError).

## Done when

- [ ] Probes `sets`, `dictcomp`, `forelse`, `assertst` match python3.
