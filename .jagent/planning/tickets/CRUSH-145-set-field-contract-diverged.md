# CRUSH-145 — SET_FIELD contract diverged: map literals broken on FastVM, JIT and AOT

| Field | Value |
|-------|-------|
| **ID** | CRUSH-145 |
| **Priority** | P1 |
| **Status** | Done (2026-10-05, branch `claude/crush-134-any-field-access`) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Found while testing CRUSH-134 on every backend. CVM1's `SET_FIELD` pops the value and the map, inserts, and **pushes the map back**; the compiler's object-literal lowering relies on that (`NEW_OBJ; <v>; SET_FIELD k; STORE` — no `dup`). FastVM, the JIT and both AOT backends did not push it back, so any map literal was broken off CVM1:

```crush
fn main() { let m = {"pos": 42} return m.pos }
```

CVM1 → 42; FastVM → `StackUnderflow`; AOT Rust/C → `null`. Separately, `m.x = v` as a statement left the pushed-back map on CVM1's stack (a leak per write).

## Resolution

- FastVM `SetField`, JIT `OP_SET_FIELD`, AOT Rust and AOT C `set_field` push the map back (CVM1's contract).
- `Statement::SetField` compiles to `<target>; <value>; set_field; pop`.
- JIT unit test `test_new_obj_set_get_field` drops the `Dup` that worked around the old behaviour.
- Tests: `differential_aot.rs` `aot_set_field_object_literals_and_statements` (multi-key and nested literals, 5000 field writes in a loop — fails without the `pop` with a JIT stack overflow) and `aot_field_access_on_any_is_dynamic` (fails without the push-back), both strict on every backend.
