# CRUSH-134 — Type checker rejects field access on `any` (params, nested maps) and `any` in conditions

| Field | Value |
|-------|-------|
| **ID** | CRUSH-134 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
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

- [ ] `fn f(p) { return p.pos }` works with a map argument
- [ ] `m.outer.inner` works
- [ ] `if m.flag` works for a bool value (runtime check)

## Technical approach

- Treat `GetField` on `any` as dynamic (result `any`); allow `any` in condition position with a runtime truthiness/bool check.

## Files to modify

- `crates/crush-frontend/src/semantics.rs`

## Decision (owner interview, 2026-10-05)

- Field access on `any` (params, nested maps) is a dynamic lookup typed `any`; a missing key gives `null` — what the VMs already do.
- `if`/`while`/`&&`/`!` accept `any` with runtime truthiness; a value statically known not to be bool (`if 5`) stays a compile error.
- Recorded in `.dejavue/decisions.md` (2026-10-05).
