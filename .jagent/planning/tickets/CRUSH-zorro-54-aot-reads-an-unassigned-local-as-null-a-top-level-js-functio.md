# CRUSH-zorro-54 — AOT reads an unassigned local as null: a top-level JS function reading a script variable prints wrong output (swc path)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-54 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none. **Must land before CRUSH-250 step 6** (a gate item there) |
| **Estimated effort** | M |

## Problem

On the swc lowering (the `.js` default until CRUSH-250 step 6), a top-level `function` is its own crush function and can't see the script's variables. When it reads one, the VM fails loudly ("load from uninitialised slot"), but **AOT Rust and C read the unassigned local as `null` and print a wrong result**:
- `fn_script_vars` prints `0 []` (node: `61 [ 3, 3, 55 ]`);
- `fn_script_late` prints `null: null` (node: `count: 2`).

Both are listed in `RUST_SILENT` in `crates/crush-aot/tests/js_parity.rs`, keyed to this ticket. This is a pre-existing bug, found by CRUSH-250's corpus programs.

**Why it isn't refused at walk time on swc.** The same pattern, a function that reads and writes a script variable, also appears in `ops_ternary_nested`, where the function is never called. That program matches node on main on the VM and on AOT. A static walk-time refusal can't tell the two apart, and it would turn `ops_ternary_nested` loud on the default path.

## The fix

**AOT must treat a read of an unassigned local the way the VM does: a run-time error ("load from uninitialised slot"), never `null`.** This fixes the whole class, on both JS lowerings and for every walker, so the pins can be removed. Decided 2026-10-15: keep the `RUST_SILENT` pins until then. A walk-time refusal on the swc lowering was rejected, because it would also turn `ops_ternary_nested` loud, and that program matches node on every engine on main.

## Success criteria

- [ ] AOT Rust and C fail at run time on a read of a local that has no value yet, with the VM's message ("load from uninitialised slot"). A read that is provably unreachable from any store may fail at compile time instead.
- [ ] `fn_script_vars` / `fn_script_late` leave `RUST_SILENT` (loud on AOT with swc), and `ops_ternary_nested` keeps matching (its function never runs).
- [ ] Lands before CRUSH-250 step 6.

## Technical approach

- In the AOT backends (`crates/crush-aot/src/codegen.rs`, `codegen_c.rs`), give non-parameter locals an "unassigned" sentinel and check it on `load`. Keep the cost low by checking only loads not dominated by a store in the same function.
- Retiring swc for `.js` (CRUSH-250 step 6) is not a substitute. The class isn't JS-specific: any walker whose output reads a local before assigning it gets `null` on AOT.

## Non-goals

- JS semantics for script variables on swc; the boa lowering owns that (CRUSH-250).
