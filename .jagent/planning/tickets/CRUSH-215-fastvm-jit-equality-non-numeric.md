# CRUSH-215 — FastVM/JIT `==`/`!=` wrong for strings, bools and null (JIT tictactoe ends immediately)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-215 |
| **Priority** | P0 |
| **Status** | Done |
| **Phase** | M1 |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`crates/crush-vm/src/fastvm/operations.rs:~66-90` (`compare_op`): strings fall through
to `_ => false` for both `Eq` and `Ne`; the Bool arm computes `x == y` even for `Ne`.

```crush
fn id(x) { return x }
print("ab" == "ab")            // FastVM: false
print(id(true) != id(false))   // FastVM: false
print(id(null) != id(1))       // FastVM: false
```

JIT: string `==` always false, `!=` always true — JIT tictactoe immediately prints
`game over: - wins!` (`w != "-"`); JIT snake output is wrong. CRUSH-136 covered string
*ordering*, not equality.

## Success criteria

- [x] Equality for every value kind shared by interp, FastVM and JIT (one implementation).
- [x] Differential test over all value-kind pairs for `==`/`!=`.

## Resolution (2026-10-30, `agent/nixp/CRUSH-215`)

One canonical equality now backs both fast engines. `crush_vm::value::runtime_value_eq`
(`crates/crush-vm/src/value.rs`) mirrors the interpreter's `Value::PartialEq`
(`crate::vm`): cross-type `Int`/`Float` numeric equality, no `Bool` coercion,
strings by value whether inline or arena-allocated, arrays/tuples/lists/
vectors/sets element-wise, maps order-insensitively, and structs
(`Object::Object`) by fields only (class name ignored, and equal to a `Map`
with the same fields — the interpreter treats struct instances as maps).

- **FastVM**: `FastOp::Eq`/`Ne` (`fastvm/execution.rs`) call `runtime_value_eq`
  with the arena; the old arena-blind `compare_op` (which fell through to
  `_ => false` for every non-numeric kind and computed `x == y` even for `!=`)
  is deleted.
- **JIT**: `Eq`/`Ne` (`crush-jit/src/compiler.rs`) lower to a new
  `OP_CMP_EQUAL` runtime helper (`crush-jit/src/runtime.rs`) that converts the
  operands and calls the same `runtime_value_eq`. The old inline `do_cmp`
  (which bitcast NaN-boxed strings/bools/refs and so reported `==` false and
  `!=` true for equal strings) is removed.

### Evidence

- `crates/crush-aot/tests/backend_agreement.rs::equality_agrees_across_every_engine`
  runs 15 programs on all six engines (scheduler VM, portable VM, FastVM, JIT,
  AOT Rust, AOT C) and requires agreement. Verified RED before the fix
  (`fastvm`/`jit` returned `Bool(false)` for `"ab" == "ab"` while the VMs and
  AOT returned true) and GREEN after.
- `crates/crush-vm/src/value.rs::eq_tests` — 4 unit tests over the value kinds
  (scalars/non-coercion/NaN, inline-vs-arena strings, arrays/maps, structs).
- `cargo test --workspace --lib --tests`: 2544 passed, 0 failed.
- `cargo test -p crush-jit --lib`: 102 passed (the existing `test_cmp_*` JIT
  cases still pass through the helper). No new clippy warnings.
