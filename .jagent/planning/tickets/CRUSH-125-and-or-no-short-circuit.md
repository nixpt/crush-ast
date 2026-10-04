# CRUSH-125 — `&&` / `||` do not short-circuit

| Field | Value |
|-------|-------|
| **ID** | CRUSH-125 |
| **Priority** | P1 |
| **Status** | Done (2026-10-04, branch `claude/crush-125-short-circuit`) |
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

- [x] `false && f()` / `true || f()` never call `f` on CVM1, FastVM, JIT and both AOT backends
- [x] the repro prints `false`
- [x] differential test across backends

## Technical approach

- Lower `a && b` to `a; dup; jz end; pop; b; end:` (and the `jnz` mirror for `||`) in the compiler, so no backend needs to change.
- Keep the result a bool (or document truthy-value semantics — decide and test).

## Files to modify

- `crates/crush-frontend/src/compiler.rs`
- backend differential tests

## Resolution

Reproduced on `main` `a8247af` first. `Compiler::compile_short_circuit` (`crush-frontend/src/compiler.rs`) lowers `&&`/`||` to `jmp_if_not`/`jmp` with a bool pushed at the join point — the result stays a bool (the open question above), and no backend changed. Live: the #65 repro prints `false`; an evaluation-order trace shows each right operand runs only when needed.

Tests: `crush-lang-sdk/tests/gh_issue_65_short_circuit.rs`; `logical_and_or_lower_to_short_circuit_branches` in `crush-frontend/tests/compiler_tests.rs`; three all-backend tests in `crush-aot/tests/differential_aot.rs` (including a branch taken mid-expression) with pinned values — needed because FastVM's out-of-range index returns null instead of trapping, so with eager evaluation every backend still "agreed".

Found while testing (pre-existing on `main`, filed): CRUSH-138 (FastVM binds call arguments in reverse order), CRUSH-139 (JIT returns the wrong branch for `if inside && !outside`).
