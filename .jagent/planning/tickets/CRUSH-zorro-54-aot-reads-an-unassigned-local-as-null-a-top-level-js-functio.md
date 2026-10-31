# CRUSH-zorro-54 — AOT reads an unassigned local as null: a top-level JS function reading a script variable prints wrong output (swc path)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-54 |
| **Priority** | P1 |
| **Status** | Done — PR #248 merged 2026-10-16 |
| **Phase** | M6 |
| **Assignee** | vega |
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

- [x] AOT Rust and C fail at run time on a read of a local that has no value yet, with the VM's message ("load from uninitialised slot"). A read that is provably unreachable from any store may fail at compile time instead.
- [x] `fn_script_vars` / `fn_script_late` leave `RUST_SILENT` (loud on AOT with swc), and `ops_ternary_nested` keeps matching (its function never runs).
- [ ] Lands before CRUSH-250 step 6.

## Technical approach

- In the AOT backends (`crates/crush-aot/src/codegen.rs`, `codegen_c.rs`), give non-parameter locals an "unassigned" sentinel and check it on `load`. Keep the cost low by checking only loads not dominated by a store in the same function.
- Retiring swc for `.js` (CRUSH-250 step 6) is not a substitute. The class isn't JS-specific: any walker whose output reads a local before assigning it gets `null` on AOT.

## Non-goals

- JS semantics for script variables on swc; the boa lowering owns that (CRUSH-250).

## Resolution

**Measured first** (a CASM read of a local that only an untaken branch assigns,
`crates/crush-aot/tests/backend_agreement.rs`): the scheduler VM and portable VM
failed with `load from uninitialised slot N`; **FastVM, the JIT, AOT Rust and
AOT C all returned `null`.** FastVM and the JIT are filed as CRUSH-zorro-55.

**Fix (AOT Rust and C).** `crates/crush-aot/src/assigned.rs` is a forward
"must be assigned" data-flow pass over the control flow the backends emit
(`jmp` / `jmp_if` / `jmp_if_not` branch, `ret` / `halt` end, everything else
falls through). A `load` that a `store` to the same name precedes on every
path from the entry is emitted as before; only the others are checked. Loads
in code the pass finds unreachable are checked too (conservative).

- **Rust:** locals are a `HashMap`, so a local not stored yet is simply absent
  — that is the sentinel. A checked load is `match locals.get(..) { Some(v) =>
  v.clone(), None => uninit_load(..) }`, the same lookup as before.
- **C:** locals frames start as a new `TAG_UNSET` value (was `mk_null()`); a
  checked load tests the tag. `TAG_UNSET` never reaches the operand stack, and
  the collector ignores it. A typed (`int64_t`/`double`, CRUSH-257) local is
  never read unassigned: inference already sees such a read as `Null` and keeps
  the local boxed, and `discover_locals` now also forces any local with a
  checked load to stay a `Value`.

Both exit 1 with `crush(aot[c]): load from uninitialised slot `<name>``.

**Harness.** `js_parity.rs` (swc): `fn_script_late`, `fn_script_vars`,
`misc_console_multi`, `ops_nan` moved from `RUST_SILENT` to `RUST_LOUD`
(the last two read the unbound names `undefined` / `NaN` as unassigned locals),
and `ops_null_eq_undefined`, which matched only because `undefined` read as
`null`, joined `RUST_LOUD`. `ops_ternary_nested` still matches. Per backend on
swc: match/loud/silent 62/51/6 → 61/56/2; boa unchanged at 67/51/1.
`python_parity.rs` and `node_corpus_test.rs` lists didn't change.

The same PR fixes a harness bug: `js_parity.rs`'s `check` shadowed the backend
name with its display label before calling `compile_and_run`, so the "rust"
runs built with the C backend. The Rust backend now really runs; its lists
were already right.
