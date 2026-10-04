# CRUSH-136 — `<` / `>` on strings type-check but fail at run time

| Field | Value |
|-------|-------|
| **ID** | CRUSH-136 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#76](https://github.com/nixpt/crush-ast/issues/76) (filed 2026-10-04 by pranix) |

## Problem

`"a" < "b"` passes the checker and fails in `compare_values` (`portable_vm.rs` ~l.563) with "expected numeric, got str".

## Reproduction

From [#76](https://github.com/nixpt/crush-ast/issues/76) (verbatim):

#### Summary

`"a" < "b"` compiles cleanly but fails at runtime with `[runtime] type error: expected numeric, got str`. `==` and `!=` work fine on strings.

#### Repro

```crush
fn main() {
  if "a" < "b" { print("lt") }
}
```

#### Expected

Either support lexicographic comparison for `<`/`>`/`<=`/`>=` on strings, or reject it at compile time. A runtime type error for code that type-checks is the worst option.

## Success criteria

- [ ] either lexicographic comparison on all backends, or a compile-time error — no run-time type error for code that type-checks

## Technical approach

- Extend `compare_values` to `Str × Str` (byte-wise/lexicographic) and match on FastVM/JIT/AOT.

## Files to modify

- `crates/crush-vm/src/arithmetic.rs`
- backend equivalents
