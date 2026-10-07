# CRUSH-166 — Design note: guest→host capability callbacks during `EXEC_LANG`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-166 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~15 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`EXEC_LANG` is one-shot (stdin vars in, sentinel JSON out). exosphere's python worker had a bidirectional bridge protocol letting guest code call host caps mid-run. Before anyone rebuilds that, write down the design and its capability implications.

## Success criteria

- [ ] `docs/design/exec-lang-callbacks.md`: protocol sketch, how each callback is gated by `HostCaps`, interaction with buckets/bwrap and wall-clock limits, failure modes
- [ ] a `dejavue decision` if a direction is chosen

## Technical approach

- Read-only design work.

## Files to modify

- `docs/design/exec-lang-callbacks.md`

## Non-goals

- implementation

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/runtimes/python/src/{worker.rs,bin/python_worker.rs}`
