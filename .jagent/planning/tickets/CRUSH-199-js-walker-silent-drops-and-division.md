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
- `.length` — fixed by CRUSH-260 (lowers to `len` on strings/arrays). String methods, classes and `o.x += 1` fail.
- Ternary: see CRUSH-196.

### Added 2026-10-12 (found while doing CRUSH-260)

- **Array/object method calls** lower to an undefined dotted function: `let a=[]; a.push(7)` →
  `Undefined function: a.push` (receiver dropped; see also CRUSH-251 for method calls with `this`).
  At minimum `push`/`pop` on arrays should lower to the existing array ops.
- **`crush_frontend::compile_cast` rejects the walker's own lowering helpers.** The JS walker emits
  `__crush_assign__` (assignment expressions) and `make_range` (C-style `for`), which
  `Compiler::compile` understands (the walker pipeline: `crush-walk-run`, `crush-aotc`), but
  `compile_cast` runs the semantic check first and fails with `Undefined function: __crush_assign__` /
  `make_range` — so even `let i = 0; while (i < 3) { i = i + 1 }` fails through that entry point. Either
  the semantic pass knows the helpers, or the walker lowers to plain `Statement::Assign` / range loops.
- **JS numbers:** `/` as float division needs JS number printing too (integer-valued floats print without
  `.0`: `10/2` → `5`), or exact divisions regress from `5` to `5.0`.

## Success criteria

- [ ] `switch`, blocks and `finally` lowered (or rejected loudly).
- [ ] JS `/` is float division, with JS number printing (`10/2` prints `5`, `10/4` prints `2.5`).
- [ ] `a.push(x)` / `a.pop()` work on arrays.
- [ ] `compile_cast` accepts the walker's output (assignments and C-style `for` compile through both entry points).
- [ ] Output tests against `node`.
