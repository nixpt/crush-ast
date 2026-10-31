# CRUSH-166 — Design note: guest→host capability callbacks during `EXEC_LANG`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-166 |
| **Priority** | P4 |
| **Status** | Done (2026-10-31) — `docs/design/exec-lang-callbacks.md` |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~15 turns) |
| **Relay lane** | F — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

`EXEC_LANG` is one-shot (stdin vars in, sentinel JSON out). exosphere's python worker had a bidirectional bridge protocol letting guest code call host caps mid-run. Before anyone rebuilds that, write down the design and its capability implications.

## Success criteria

- [x] `docs/design/exec-lang-callbacks.md`: protocol sketch, how each callback is gated by `HostCaps`, interaction with buckets/bwrap and wall-clock limits, failure modes
- [x] a `dejavue decision` if a direction is chosen — none chosen: the note recommends not building it until a consumer needs it

## Technical approach

- Read-only design work.

## Files to modify

- `docs/design/exec-lang-callbacks.md`

## Non-goals

- implementation

## Source (reference only — re-implement, don't copy)

- exo `crates/platform/runtimes/python/src/{worker.rs,bin/python_worker.rs}`

## Resolution (2026-10-31)

Written from exosphere `crates/platform/runtimes/python` @ `0c35ddec` (retired in EXO-216, so this
captures it before it is deleted). Found while reading it: exosphere's bridge was inert in its
default configuration — its context is a `thread_local!` set on the calling thread, but the
default `timeout_ms = 5000` runs the guest on a new thread, where the injected functions fall into
"local-only mode" and report success without reaching the host. The note's §3 proposal (separate fd
pair, one `call` message, per-block declared capabilities through the existing grant check,
whole-block deadline) is a recommendation, not a decision; build only when a consumer needs it.
