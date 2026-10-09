# CRUSH-183 — Host caps that return nothing (`fs.write`, `akg.write`, `message_bus.*`, `task.stop`) fail with stack underflow

| Field | Value |
|-------|-------|
| **ID** | CRUSH-183 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`fs.write("w.txt", "x")` writes the file and then fails with
`[runtime] stack underflow`. The same happens for `akg.write`,
`message_bus.publish`, `message_bus.subscribe` and `task.stop`, which makes
`--bus` and `--akg` unusable from Crush source.

`cap_returns_value()` only consults `crush_vm::capabilities()`; host caps are not
in that list, so it defaults to `true` and the compiler emits a `POP` for a value
that was never pushed.

## Where

`crates/crush-lang-sdk/src/compile.rs:22` (`cap_returns_value`).

## Success criteria

- [ ] Every host cap either always pushes a value (Null) or is known to the compiler
      as void, from one source of truth (e.g. `effects::catalog()`).
- [ ] Test: each listed cap used as a statement runs to completion.
