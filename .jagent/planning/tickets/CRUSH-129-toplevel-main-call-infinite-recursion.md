# CRUSH-129 — Top-level `main()` after `fn main(){}` recurses forever

| Field | Value |
|-------|-------|
| **ID** | CRUSH-129 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#69](https://github.com/nixpt/crush-ast/issues/69) (filed 2026-10-04 by pranix) |

## Problem

Script-style top-level statements are merged into the front of an explicit `fn main` body (`crates/crush-frontend/src/parser/mod.rs` ~l.521, the CRUSH-12 fix). A top-level `main()` call therefore becomes `main` calling itself.

## Reproduction

From [#69](https://github.com/nixpt/crush-ast/issues/69) (verbatim):

#### Summary

A stray `main()` call after the `fn main() { ... }` definition compiles cleanly but produces infinite self-recursion — at runtime it dies with call-depth-quota exceeded instead of running once.

#### Repro

```crush
fn main() { print("hi") }
main()
```

Expected: prints `hi` once (or a compile error for the stray call). Actual: infinite recursion until the call-depth quota trips.

#### Notes

Easy to hit by accident (e.g. habits from other languages). A warning or error at compile time would save debugging.

## Success criteria

- [ ] the repro prints `hi` once
- [ ] a top-level `main()` call with an explicit `fn main` is dropped with a warning (or rejected) — decide, document, test

## Technical approach

- When merging, drop top-level `ExprStmt(Call main())` statements if `fn main` exists, and surface a diagnostic.

## Files to modify

- `crates/crush-frontend/src/parser/mod.rs`
