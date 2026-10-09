# CRUSH-182 — `--task` spawns arbitrary processes without `--process`; children outlive the VM

| Field | Value |
|-------|-------|
| **ID** | CRUSH-182 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`task.start("t1", "sleep", "333")` under `--task` runs `sleep 333`, still running
after `crush-run` exits. With `touch` it creates files. The `--task` help text
never mentions process spawning (the effects catalog does list `process/spawn`).

## Success criteria

- [ ] `task.start` requires the `process` grant too (or `--task` help says plainly
      that it grants process spawning).
- [ ] Children are killed (or explicitly detached, opt-in) when the VM exits.
