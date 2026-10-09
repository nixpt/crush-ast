# CRUSH-195 — `crush-repl` re-runs the whole session on every line (side effects repeat; one error poisons the session)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-195 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Piping `io.print("a")` then `io.print("b")` into the REPL prints `a`, `a`, `b`.
Each line recompiles and re-executes the accumulated program, so side effects
repeat, and a single failing line (e.g. a refused cap) makes every later line fail.

## Success criteria

- [ ] The REPL keeps VM state between lines and executes only the new input (or
      replays only declarations, not effects).
- [ ] A failing line is dropped from the session.
