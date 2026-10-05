# CRUSH-134 — Type checker rejects field access on `any` (params, nested maps) and `any` in conditions

| Field | Value |
|-------|-------|
| **ID** | CRUSH-134 |
| **Priority** | P2 |
| **Status** | Done (2026-10-05, branch `claude/crush-134-any-field-access`) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | M |
| **GitHub** | [#74](https://github.com/nixpt/crush-ast/issues/74), [#77](https://github.com/nixpt/crush-ast/issues/77) (filed 2026-10-04 by pranix) |

## Problem

Map field access yields `any`, and the checker rejects `.field` on `any` and `any` in `if`. So maps can't cross a function boundary or be nested more than one level (`m.a.b`), and `if m.flag` needs `== true`. Language-design decision needed: dynamic lookup on `any` (runtime error on a missing key) is the obvious answer.

## Reproduction

From [#74](https://github.com/nixpt/crush-ast/issues/74) (verbatim):

#### Summary

Two related type-checker papercuts around map field access:

1. Field access on a function **parameter** is rejected even for plain maps:

```crush
fn getpos(p) { return p.pos }
fn main() {
  let m = {"pos": 42}
  print(getpos(m))
}
```

`[E-PP02] Cannot access field 'pos' on non-struct type any`. (Structs do not exist in the runtime, so there is no way to satisfy the checker — maps cannot cross function boundaries as anything but opaque `any`.)

2. Map field access yields type `any`, so using it directly as a condition fails:

```crush
let m = {"flag": true}
if m.flag { print("on") }
```

`[type] type error: If condition must be bool, found any`. Workaround: `if m.flag == true`.

#### Expected

At minimum, clearer errors. Ideally: allow field access on `any`-typed params (dynamic lookup, runtime error on missing key), and treat a map field known to hold a bool as a bool in condition position.

From [#77](https://github.com/nixpt/crush-ast/issues/77) (verbatim):

#### Summary

`m.outer.inner` fails with `Cannot access field 'inner' on non-struct type any`, even when `m` is a local variable. Binding the intermediate to a local does not help:

```crush
fn main() {
  let m = {"outer": {"inner": 42}}
  let o = m.outer
  print(o.inner)
}
```

`[type] type error: Cannot access field 'inner' on non-struct type any`.

#### Notes

Array indexing on the `any` result works (`arr[i]`), but field access does not. This makes nested maps unusable for anything beyond one level of dot access. Related to #74 (field access on params), but this reproduces with plain locals.

## Success criteria

- [x] `fn f(p) { return p.pos }` works with a map argument
- [x] `m.outer.inner` works
- [x] `if m.flag` works for a bool value (runtime check)

## Technical approach

- Treat `GetField` on `any` as dynamic (result `any`); allow `any` in condition position with a runtime truthiness/bool check.

## Files to modify

- `crates/crush-frontend/src/semantics.rs`

## Decision (owner interview, 2026-10-05)

- Field access on `any` (params, nested maps) is a dynamic lookup typed `any`; a missing key gives `null` — what the VMs already do.
- `if`/`while`/`&&`/`!` accept `any` with runtime truthiness; a value statically known not to be bool (`if 5`) stays a compile error.
- Recorded in `.dejavue/decisions.md` (2026-10-05).

## Resolution

- **Type checker** (`semantics.rs`): `.field` on `any` (as on a map) is typed `any`. Every `if`/`while` condition goes through `check_condition`, which accepts `bool` and `any` and rejects anything else (`if 5`, `while "s"` stay compile errors). `&&`/`||`/`!` already accepted `any`.
- **One truthiness rule on every backend** (decision 2026-10-05: CVM1's): `null`, `false`, `0`, `0.0`, `""` and empty collections are falsy, everything else truthy. Accepting `any` conditions made the backends' disagreement reachable from source, so they were aligned:
  - CVM1: `Value::is_truthy` (scheduler) — PortableVm's identical copy now calls it.
  - FastVM: `is_truthy` takes the arena so strings and collections are checked for emptiness (was: `0`, `0.0`, `""`, `[]` all truthy).
  - JIT: immediates decided inline (`0` and `±0.0` now falsy); refs go through the new `OP_TRUTHY` helper. `JitValue::is_truthy` follows.
  - AOT Rust: `truthy` adds floats, strings and maps. AOT C (crush-aot): `_truthy` adds floats, strings, arrays, objects. crush-aotc: `cv_truthy` adds floats and strings.
- **Tests:** `crush-lang-sdk/tests/gh_issue_74_77_field_access_on_any.rs` (the issues' repros, missing key → null, `while` on `any`, `if 5` still rejected); `differential_aot.rs` `aot_truthiness_is_canonical`, `aot_truthiness_through_logical_ops`, `aot_field_access_on_any_is_dynamic`, `aot_set_field_object_literals_and_statements` (strict on every backend incl. the JIT); crush-aotc `truthiness_is_canonical`. Each fails without its fix.
- Found on the way: map literals were broken on every backend but CVM1 (SET_FIELD contract) — CRUSH-145, fixed here; crush-aotc never got #76's string ordering and isn't in the differential harness — CRUSH-144.
