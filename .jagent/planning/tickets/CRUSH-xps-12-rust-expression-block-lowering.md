# CRUSH-xps-12 — Preserve Rust expression blocks or reject unsupported forms

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-12 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5, CRUSH-197 |
| **Estimated effort** | M |

## Problem

Rust block and return expressions are not lowered through a complete CAST
contract. `block_to_expr` inspects only the first statement, so later
statements can be dropped. Local declarations and expression-position returns
can emit `__crush_let__` and `__crush_return__`, which have no matching
compiler intrinsic and can surface later as undefined-function errors. The
lowering must not silently accept these forms or defer the failure to runtime.

## Goal

Make every supported Rust expression block preserve its value and effects, and
make unsupported block/return forms fail at the Rust frontend boundary with a
clear diagnostic.

## Scope

- In: `if` expressions, block expressions, statement sequencing in expression
  blocks, and expression-position `return` lowering.
- Out: adding new Rust syntax or changing the Crush CAST schema unless a
  separately reviewed schema change proves necessary.

## Acceptance

- [ ] Multi-statement block expressions either lower all statements in order
      and preserve the tail value, or fail with a source-located unsupported
      construct error; no statements are dropped.
- [ ] Rust `if` expression fixtures compare exactly with `rustc` for supported
      branch forms.
- [ ] Expression-position `return` is represented using existing CAST
      statements where possible; otherwise it is rejected before semantic
      analysis/code generation.
- [ ] Every `__crush_*` helper emitted by the Rust frontend has a documented
      compiler intrinsic contract, or is removed in favor of native CAST.
- [ ] The supported subset and exact-or-loud corpus cover these boundaries.

## Notes

- The corresponding helper emissions are in `crates/crush-lang-rust/src/lower_expr.rs`.
- This is a Rust-specific lowering follow-up to the broader exact-or-loud gate
  in CRUSH-xps-5.
