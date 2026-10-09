# CRUSH-199 — JS walker: `switch`, bare blocks and `finally` silently dropped; `/` is integer division

| Field | Value |
|-------|-------|
| **ID** | CRUSH-199 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

In `crates/crush-lang-js/src/lower_swc.rs`:

- `switch` (~572-612) builds its case list then discards it and emits `NullLiteral`:
  `switch(2){case 2: console.log("two")}` prints nothing.
- Bare block `Stmt::Block(_) => return Ok(None)` (~620):
  `let x=1; { console.log("in"); x=2; } console.log(x)` prints `1` (expected `in`, `2`).
- `finalizer: _` (~549): `try{throw "s"}catch(e){} finally{console.log("fin")}` never prints `fin`.
- `10/4` → `2` (expected `2.5`): JS numbers are floats, integer literals hit the VM's C-style division.
- `.length`, string methods, classes and `o.x += 1` fail.
- Ternary: see CRUSH-196.

## Success criteria

- [ ] `switch`, blocks and `finally` lowered (or rejected loudly).
- [ ] JS `/` is float division.
- [ ] Output tests against `node`.
