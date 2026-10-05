# CRUSH-143 — Optimizer drops assignments made inside `if` branches

| Field | Value |
|-------|-------|
| **ID** | CRUSH-143 |
| **Priority** | P0 |
| **Status** | Done (2026-10-05, branch `claude/crush-136-string-compare`) |
| **Phase** | M1 |
| **Assignee** | claude |
| **Dependencies** | none |
| **Estimated effort** | XS |

## Problem

After an `if`, the optimizer restored the constants it knew *before* the `if` and ignored assignments made in either branch. So a variable assigned in a branch was still treated as its old constant afterwards and folded away:

```crush
fn t() { return true }
fn main() {
  let n = 0
  if t() { n = n + 1 }
  print(n)        // printed 0
}
```

Any counter or flag updated conditionally was wrong under `crush-run x.crush`, which always optimizes. `while` already invalidated the variables it mutates; `if` never did. Found while writing CRUSH-136's all-backend test, where every backend agreed on the wrong answer `0`.

## Success criteria

- [x] the repro prints `1`; then-, else- and nested-branch assignments survive the `if`
- [x] a condition folded to a literal keeps that branch's constants

## Resolution

`Optimizer` `If` arm:
- **Condition folded to a literal:** keeps the surviving branch's constants.
- **Otherwise:** drops every variable either branch mutates.

`collect_mutated_vars` also counts `let` in nested blocks and a `@lang` block's write-back variable. Test: `crush-lang-sdk/tests/optimizer_if_branches.rs`, which fails without the fix.
