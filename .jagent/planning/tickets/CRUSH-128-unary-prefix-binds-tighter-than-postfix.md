# CRUSH-128 — Prefix `!` / `-` bind tighter than call, index and field access

| Field | Value |
|-------|-------|
| **ID** | CRUSH-128 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#68](https://github.com/nixpt/crush-ast/issues/68) (filed 2026-10-04 by pranix) |

## Problem

`Token::Not` / `Token::Minus` are handled in `parse_primary` with `operand = parse_primary()` (`crates/crush-frontend/src/parser/mod.rs` ~l.1540–1560); the postfix call is then applied to the `UnaryOp`, so `!is_digit(x)` is `(!is_digit)(x)` → "Cannot call non-function". Triage also confirmed `-three()` fails the same way.

## Reproduction

From [#68](https://github.com/nixpt/crush-ast/issues/68) (verbatim):

#### Summary

`!` as a logical-not prefix works on some expressions but is rejected on call expressions with a confusing error.

#### Repro

```crush
fn is_digit(c) { return c == "5" }
fn main() {
  if !is_digit("x") { print("not a digit") }
}
```

Compile error: `[E-PP01] Cannot call non-function` pointing at the `!`.

#### Expected

`!expr` should work uniformly, or the error should say what is actually wrong. (Workaround: `if is_digit(x) { } else { ... }`.)

## Success criteria

- [ ] `!f(x)`, `-f(x)`, `!a[i]`, `!m.flag`, `-a[i]` parse as `!(f(x))` etc.
- [ ] `-2 * 3` and `!a && b` keep their precedence

## Technical approach

- Parse the unary operand with the postfix chain (call / index / field), not a bare primary.

## Files to modify

- `crates/crush-frontend/src/parser/mod.rs`
