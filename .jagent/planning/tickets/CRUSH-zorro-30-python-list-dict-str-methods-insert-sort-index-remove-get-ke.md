# CRUSH-zorro-30 — Python: list/dict/str methods — insert/sort/index/remove, get/keys/values/items, find, str * int

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-30 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 for `sort(key=)` |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, Python `.py` probe (50 programs vs python3, main `83f6599`) |

## Problem

Method calls the walker doesn't know are compile errors (Probe programs: `/workspace/scratch/pyprobe/` (vega's box); run with `crush-walk-run <file>.py`.):
- list: `.insert(i, x)`, `.sort()`, `.index(x)` (`.remove(x)` likely too); `reverse=True` on `sorted`.
- dict: `.get(k, default)`, `.keys()`, `.values()`, `.items()` — `for k, v in d.items()` is everywhere.
- str: `.find(s)`; `"ab" * 3` / `"=" * 5` → runtime `type error: expected numeric, got str`.

## Fix

Extend `lower_method` (`crates/crush-lang-python/src/lower_expr.rs`) with these, reusing
`collections.keys/values/entries` (insertion order since #188) and adding `py.*` caps where nothing
exists. `str * int` / `list * int` → a repeat cap when an operand is a string/list (runtime dispatch if
types are unknown).

## Done when

- [ ] Probe programs `listops`, `listsort`, `dictops`, `dictkv`, `strmeth`, `strmul` match python3.
