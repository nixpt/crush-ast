# CRUSH-125 — `&&` / `||` do not short-circuit

| Field | Value |
|-------|-------|
| **ID** | CRUSH-125 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **GitHub** | [#65](https://github.com/nixpt/crush-ast/issues/65) (filed 2026-10-04 by pranix) |

## Problem

The compiler emits eager `and`/`or` (`crates/crush-frontend/src/compiler.rs` ~l.1336) after evaluating both operands, and every backend's AND/OR pops two values (`portable_vm.rs` ~l.571). The bounds-check idiom `i < len(s) && s[i] == x` therefore indexes out of range.

## Reproduction

From [#65](https://github.com/nixpt/crush-ast/issues/65) (verbatim):

#### Summary

`&&` and `||` evaluate both operands unconditionally. There is no short-circuit evaluation, so a bounds check like `i < len(s) && s[i] == "x"` indexes out of bounds when `i >= len(s)`.

#### Repro

```crush
fn main() {
  let s = "abc"
  let i = 3
  if i < len(s) && s[i] == "x" { print("true") } else { print("false") }
}
```

Expected: `false`. Actual: `[runtime] array index out of range: 3 (len 3)`.

#### Expected

`&&` should not evaluate the right operand when the left is false; `||` should not evaluate the right operand when the left is true, per every mainstream language. At minimum this is a soundness trap: the idiomatic bounds-check pattern is broken.

## Success criteria

- [ ] `false && f()` / `true || f()` never call `f` on CVM1, FastVM, JIT and both AOT backends
- [ ] the repro prints `false`
- [ ] differential test across backends

## Technical approach

- Lower `a && b` to `a; dup; jz end; pop; b; end:` (and the `jnz` mirror for `||`) in the compiler, so no backend needs to change.
- Keep the result a bool (or document truthy-value semantics — decide and test).

## Files to modify

- `crates/crush-frontend/src/compiler.rs`
- backend differential tests
