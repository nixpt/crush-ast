# CRUSH-186 — String `+` is limited by the *output* quota; wall-clock limit doesn't bound `process.exec`/loops

| Field | Value |
|-------|-------|
| **ID** | CRUSH-186 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- Building any string over 1 MB with `+` fails with `output quota exceeded` even if
  nothing is printed (`crates/crush-vm/src/scheduler.rs:~648`), while `str.concat` is
  unlimited. The quota is the wrong one; a memory quota (CRUSH-181) should cover both.
- `max_wall_time_ms` (30 s) is not enforced across a blocking `process.exec("sleep","40")`
  or a plain `while true` with a large `--max-steps` (ran >60 s). Related: CRUSH-19/M7.

## Success criteria

- [ ] `+` and `str.*` share one size/memory quota; output quota applies only to output.
- [ ] Wall-clock limit is checked in the step loop and applied as a timeout to blocking caps.
