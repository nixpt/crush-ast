# CRUSH-131 — Optimizer constant-folds variables across `@lang` blocks (stale write-back)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-131 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **GitHub** | [#71](https://github.com/nixpt/crush-ast/issues/71) (filed 2026-10-04 by pranix) |

## Problem

`crates/crush-frontend/src/optimizer.rs` has no `LangBlock` arm (falls through `other => vec![other]` with the known-constant map intact), so a variable written back by the guest block is still replaced by its pre-block constant. The reporter has a one-arm fix that passes all crush-frontend tests.

## Reproduction

From [#71](https://github.com/nixpt/crush-ast/issues/71) (verbatim):

#### Summary

The optimizer constant-folds a variable whose value is assigned inside a `@lang` polyglot block. After the block writes the variable back, later reads use the stale folded constant instead of the updated value.

#### Status

One-arm fix applied locally (uncommitted) in `crates/crush-frontend/src/optimizer.rs`: the `LangBlock` arm invalidates known constants on entry. All 144 `crush-frontend` tests pass with the fix. Happy to PR it if wanted — filing here so the bug is tracked regardless.

#### Expected

Variables written by a `@lang` block must not be constant-folded across the block boundary.

## Success criteria

- [ ] a variable assigned before and written by a `@lang` block is read with the written-back value after it
- [ ] regression test

## Technical approach

- `LangBlock` arm clears the known-constants map (or just its write-back variables).

## Files to modify

- `crates/crush-frontend/src/optimizer.rs`
