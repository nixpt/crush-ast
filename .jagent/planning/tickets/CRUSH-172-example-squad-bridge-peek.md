# CRUSH-172 — Example capsule: `squad-bridge-peek` (first pure-Crush capsule)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-172 |
| **Priority** | P5 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-151 |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | E3 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-capsules' `squad-bridge-peek` is the only pure-Crush capsule outside crush-ast and matches crush-pkg's manifest shape. It needs `fs.cat` (CRUSH-151) and hard-codes a personal absolute path.

## Success criteria

- [ ] `examples/crush/capsules/squad-bridge-peek/` with `capsule.toml` + `main.crush`, path taken from an argument/env grant, no personal paths
- [ ] `crush-pkg build` + `run` work on it with `--fs` scoped to a fixture file; CI or conformance covers it

## Technical approach

- Re-create, don't copy verbatim.

## Files to modify

- `examples/crush/capsules/squad-bridge-peek/`

## Non-goals

- the other crush-capsules (dead)

## Source (reference only — re-implement, don't copy)

- `nixpt/crush-capsules` `squad-bridge-peek/`
