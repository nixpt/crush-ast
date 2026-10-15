# CRUSH-xps-12 — Preserve Rust expression blocks or reject unsupported forms

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-12 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M2 / Rust parity |
| **Assignee** | nixp (user-directed) |
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

- [x] Multi-statement block expressions either lower all statements in order
      and preserve the tail value, or fail with a source-located unsupported
      construct error; no statements are dropped.
- [x] Rust `if` expression fixtures compare exactly with `rustc` for supported
      branch forms.
- [x] Expression-position `return` is represented using existing CAST
      statements where possible; otherwise it is rejected before semantic
      analysis/code generation.
- [x] Every `__crush_*` helper emitted by the Rust frontend has a documented
      compiler intrinsic contract, or is removed in favor of native CAST.
- [x] The supported subset and exact-or-loud corpus cover these boundaries.

## Notes

- The corresponding helper emissions are in `crates/crush-lang-rust/src/lower_expr.rs`.
- This is a Rust-specific lowering follow-up to the broader exact-or-loud gate
  in CRUSH-xps-5.

## Implementation and validation in PR #213

- Closure bodies use the existing function-tail statement lowering, preserving
  declarations, effects, branch tails, and explicit returns. The closure-block
  corpus matches `rustc`, including a mutable scalar capture.
- Generic value blocks accept empty blocks or a single tail expression. Other
  forms fail during Rust lowering with their source line and column; the SDK
  regression verifies a diagnostic at line 2 before VM execution.
- `__crush_let__` and `__crush_return__` are no longer emitted. Rust's remaining
  helper calls (`__crush_assign__`, `__crush_ternary__`) use compiler contracts.
- VM execution and rustc freshness checks cover this slice. Full AOT parity
  coverage remains part of CRUSH-xps-5.

## Detailed review specification (2026-10-15)

[Batch spec](../designs/rust-parity-xps-8-13.md#xps-10-12-close-the-existing-review-contracts).
The implementation remains **In review in PR #213**. The original goal is
complete supported lowering or explicit rejection; a general expression-block
IR is not required to close this ticket.

### Block and control-flow contract

| Context | Supported result |
|---|---|
| Function/closure body | Complete supported statement sequence, effects in order, lexical locals, correct tail and explicit return |
| Statement block | Complete sequence in its lexical scope; locals do not leak |
| Generic empty value block | Unit/null value with correct source identity |
| Generic single-tail value block | The tail's value and effects, evaluated once |
| Assigned if-expression | Condition once; only the selected branch executes; supported tails retain their value class |
| Generic multi-statement or semicolon-ending value block | Source-located unsupported-block diagnostic before shared codegen |
| Return used in a value expression | Supported statement representation or explicit source-located refusal; no fake call helper |

No statement may be silently dropped to accept a block. Intermediate private
markers (including xps-7 ownership markers) must be consumed or rejected before
CAST enters the shared compiler. Actual assignment/ternary intrinsics retain
the xps-10 arity, result and lazy-evaluation contract.

### Review matrix

| Case | Required proof |
|---|---|
| Body with declaration, print/effect, update and tail | Complete ordered stdout and correct result |
| Body explicit early return in one branch | Correct result and no subsequent side effects |
| Empty versus single-tail block | Unit versus tail type/result preserved; no discarded expression |
| Assigned if with effectful condition/branch tails | Condition once; nonselected branch does not execute |
| Shadowed local in nested statement block | Inner value visible only in its own scope |
| Multi-statement/semicolon-ending generic value block | Useful source coordinate, including line-2 SDK rejection regression |
| Nested expression-position return | Early source refusal, not undefined `__crush_return__` at runtime |
| Final serialized CAST | No unresolved `__crush_let__`, `__crush_return__` or private value markers |

### Review exit

- [ ] Map the matrix to existing tests or focused additions/corrections. Assert
      stdout bytes and return values; merely compiling a function body is not
      proof that every statement executes.
- [ ] Record SDK/rustc proof and both AOT exact-or-loud outcomes. Apply xps-6
      identity and xps-7 value checks rather than bypassing them for blocks.
- [ ] Keep the conservative value-block boundary documented. xps-9 may reject
      effectful format-argument repetition/reordering when it cannot store
      temporaries; it must not synthesize a closure to bypass this boundary.
- [ ] Merge PR #213 and required dependent corrections before marking Done.
      Broader expression-block acceptance requires a separately reviewed slice.

## Dispatch for review closure

Inspect `lower_expr::block_to_expr`, `lower_stmt::{lower_block,lower_tail_return}`,
CAST statement/control-flow compilation, xps-6/7 checks and SDK block fixtures.
Start with existing supported/rejected cases in PR #213. Add only missing
proof/corrections in an isolated worktree and preserve the rejection boundary
unless a new lowering design has been reviewed.
