# CRUSH-182 — `--task` spawns arbitrary processes without `--process`; children outlive the VM

| Field | Value |
|-------|-------|
| **ID** | CRUSH-182 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M7 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`task.start("t1", "sleep", "333")` under `--task` runs `sleep 333`, still running
after `crush-run` exits. With `touch` it creates files. The `--task` help text
never mentions process spawning (the effects catalog does list `process/spawn`).

## Success criteria

- [x] `task.start` requires the `process` grant too (or `--task` help says plainly
      that it grants process spawning).
- [x] Children are killed (or explicitly detached, opt-in) when the VM exits.

## Resolution (2026-10-30, `agent/nixp/CRUSH-301`)

- **Children no longer outlive the VM.** `TaskState` now implements `Drop`,
  killing and reaping every still-running task. The capability registry (and
  therefore the state) is dropped when the run ends, so a spawned `sleep 333`
  no longer survives `crush-run`. A host that wants a detached task can stop
  tracking it before the drop.
- **The grant is stated.** `--task` spawns host processes, so its help text and
  the `crush-run --help` capability list now say plainly that it grants
  `process/spawn` (which the effects catalog already listed for `task.start`).

### Evidence

- `task::tests::dropping_the_task_state_kills_children` — a child that would
  `touch` a marker after 1s is killed when the state drops, so the marker never
  appears. Verified RED (the marker appeared).
- Live: `crush-run run --task task.crush` (which starts `sleep 333`) leaves no
  `sleep` process behind (before: one lingering).
- `cargo test --workspace --lib --tests`: 2572 passed, 0 failed.
