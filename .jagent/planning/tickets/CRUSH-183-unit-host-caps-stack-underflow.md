# CRUSH-183 — Host caps that return nothing (`fs.write`, `akg.write`, `message_bus.*`, `task.stop`) fail with stack underflow

| Field | Value |
|-------|-------|
| **ID** | CRUSH-183 |
| **Priority** | P1 |
| **Status** | Done (2026-10-09) |
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

- [x] Every host cap either always pushes a value (Null) or is known to the compiler
      as void, from one source of truth (e.g. `effects::catalog()`).
- [x] Test: each listed cap used as a statement runs to completion.

## Resolution

Both VMs now push Null when a host capability returns nothing (`portable_vm.rs` and
`scheduler.rs`, the host-cap arm of `dispatch_cap`), so the compiler's POP after every
host call is always right and `let r = fs.write(..)` stores null. Built-ins (`io.print`)
keep their known-void handling. `tests/void_host_caps_test.rs` covers fs.write (statement
and value), akg.write, message_bus.publish/subscribe; `crush_run_test.rs` covers the CLI.
task.stop is fixed by the same change but not tested (task.start fails separately).
