# CRUSH-217 — Indexing, map access, `len(map)` and value printing differ on every engine

| Field | Value |
|-------|-------|
| **ID** | CRUSH-217 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

| Case | interp | FastVM | JIT | AOT |
|---|---|---|---|---|
| `s[1]` on a string | `é` (char) | — | — | `null` |
| out of bounds `a[3]` | error | `null` | `null` | `null` |
| `a[-1]` | `3` | `null` | `null` | `3` |
| `{"k":1}["k"]` | error `array index must be int, got str` | `null` | `null` | `null` |
| `m["k"] = 2` | error `expected array, got map` | ignored | ignored | ignored |
| `len(map)` | error | `TypeMismatch` | `null` | `0` |
| `"x" + 1.0` | `x1.0` | `x1` | `x1` | `x1.0` |
| `print([1,2])` | `[1, 2]` | `@259` (arena ref) | `@259` | `[array#0]` |

No engine returns `1` for the map bracket read (interp `ARR_GET` has no Map arm,
`crates/crush-vm/src/scheduler.rs:~843`; frontend gap GAP-MAP-SUBSCRIPT).
interp map/struct key order follows `HashMap` and changes between runs (CRUSH-42),
which makes `crush-diff` report random divergences.

## Success criteria

- [ ] One documented semantics for each row, implemented once and called by every
      backend (CRUSH-114 rule): maps indexable by string key; OOB is an error everywhere;
      negative index either supported everywhere or an error everywhere.
- [ ] Shared value formatter for printing (arrays/maps/structs) across engines; map
      iteration order deterministic (insertion order).
- [ ] Differential test per row.
