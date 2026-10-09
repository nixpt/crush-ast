# CRUSH-220 — `crush-diff` misses almost every cross-engine bug: FastVM abstains, JIT/AOT not included

| Field | Value |
|-------|-------|
| **ID** | CRUSH-220 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-77 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Every P0 engine bug from the 2026-10-09 sweep (CRUSH-187, 189, 213–216) was reported
as "agrees" by `crush-diff`, except the random map-order case:

- FastVM "abstains" on any program using `io.print` (no caps in the harness) — 30/38 examples.
- JIT and AOT aren't run at all (CRUSH-77 scope; still Backlog).
- `crush-aotc benchmark` warns "FastVM output Null diverges" for the same reason
  (FastVM tier runs without caps and reads `main`'s return).
- Bad paths / `--help` → exit 0 (see CRUSH-212).

## Success criteria

- [ ] FastVM and JIT get a real `io.print` (and the default stdlib) in the harness.
- [ ] AOT (gcc at least) included where the program is AOT-supported.
- [ ] Composite values compared via a canonical formatter (CRUSH-217) so map order doesn't create noise.
- [ ] The sweep's repro programs added to the corpus as regression cases.
