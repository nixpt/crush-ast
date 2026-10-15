# CRUSH-xps-10 — Validate walker intrinsics and loop scopes in semantic analysis

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-10 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M1 / SDK correctness |
| **Assignee** | nixp (user-directed) |
| **Dependencies** | CRUSH-xps-2, CRUSH-197 |
| **Estimated effort** | M |

## Problem

`crush-lang-sdk::compile_source` sends walker-produced CAST through
`compile_cast_owned`, which runs semantic analysis before compiler lowering.
Rust assignment lowers to the compiler intrinsic `__crush_assign__`, but the
semantic analyzer treats it as an ordinary call and rejects it as undefined.
The SDK CI's Rust `while` and `for` execution cases therefore fail before
code generation. The same contract can affect any walker that emits this
intrinsic. The semantic analyzer also has no `Statement::For` scope handling,
so a loop binding such as `i` is not declared while checking the loop body.

## Goal

Make semantic analysis validate the supported walker CAST forms consistently
with the frontend compiler, while continuing to reject arbitrary unknown calls.

## Scope

- In: the internal walker-intrinsic contract and `Statement::For` scope,
  including Rust SDK integration coverage.
- Out: changing Rust ownership/value semantics, adding new loop syntax, or
  allowing guest-selected host capabilities.

## Acceptance

- [x] Compiler-owned walker intrinsics are recognized by semantic analysis
      through a shared or explicitly synchronized contract; arbitrary unknown
      function calls remain errors.
- [x] Intrinsic arguments and arity/target shape are checked without treating
      compiler intrinsics as host capabilities.
- [x] `Statement::For` checks its iterable, binds its iteration variable in a
      loop-local scope, checks the body, and does not leak that binding after
      the loop.
- [x] SDK Rust `while` assignment and `for` range accumulation execute with
      exact expected output (`5` and `45`).
- [x] Any other enabled walker using `__crush_assign__` has a focused
      regression test through its supported compile path.

## Notes

- Found by the failing `Test (sdk)` jobs on [PR #213](https://github.com/nixpt/crush-ast/pull/213).
- The compiler already lowers `__crush_assign__` as an intrinsic; the defect
  is the earlier semantic-analysis gate.

## Implementation and validation in PR #213

- Semantic analysis and code generation use a shared walker-intrinsic argument
  contract; optimization preserves binding targets and loop-carried changes.
- Both semantic passes bind and check loop variables without leaking their scope.
- SDK execution tests cover assignment loops through `compile_source` for Rust,
  Python, and JavaScript, with exact output; malformed Python named-expression
  helper operands are rejected before execution.
- Rust mapped standard-library methods emit resolved named capabilities; a
  rustc-backed fixture verifies locals named `array`, `str`, and `math` do not
  change capability resolution. The host registers stdlib explicitly.
- Frontend regressions check invalid arity/targets, unknown helpers, loop scope,
  range bounds, body checking, and existing struct-field iteration.
- The shared helper check also addresses the `compile_cast` rejection described
  in CRUSH-199; that ticket's other JS lowering gaps remain separate.

## Detailed review specification (2026-10-15)

[Batch spec](../designs/rust-parity-xps-8-13.md#xps-10-12-close-the-existing-review-contracts).
This ticket's implementation remains **In review in PR #213**. The checked
acceptance above describes that implementation; these review cases define
closure evidence and any focused correction still required. Do not reset the
ticket to Backlog or build a duplicate helper system.

### Shared contract

- `walker_intrinsics::lookup` owns arity, binding-target/write behavior and
  result class. Semantic analysis, optimization and compilation agree.
- Assignment evaluates its RHS once, writes the named binding and yields unit
  (runtime null). A target is an lvalue, not an ordinary read or a foldable
  constant; RHS self-reference sees the prior runtime value.
- The loop iterable is checked/evaluated in the enclosing environment, and the
  iteration binding exists only in the body. Preserve outer bindings with the
  same spelling. Branch and loop writes invalidate unsafe constant assumptions.
- Compiler helpers are distinguished from source calls. A source function or
  local callable named like a helper must retain its source behavior or be
  rejected at the Rust boundary with an explicit reserved-name diagnostic.
  Metadata respected only by the walker is not sufficient if later compiler
  stages reinterpret the same call by name.
- Unknown helpers/calls, wrong arity, invalid lvalues and bad range operands
  remain errors. Internal helpers do not grant host capabilities.

### Review matrix

| Case | Required proof |
|---|---|
| Rust while/for accumulators | Exact `5` and `45`; no uninitialized slots |
| `x = x + 1`, compound updates, nested branches/loops | Runtime values retained after optimization; source operand evaluation once |
| Assignment used as a unit-valued argument | Unit result; assigned collection keeps its xps-7 value class |
| Loop variable shadows an outer local | Body sees the iteration binding; outer local is unchanged afterwards |
| Reference to loop-local after the loop | Semantic rejection, not a stale VM slot |
| Wrong helper arity, nonvariable assignment, unknown helper | Early validation rejection through public compilation |
| User source helper-like function/local names | Correct source dispatch or source-located refusal |
| Python/JS assignments using the shared helper contract | Supported compile paths remain correct; malformed operands still fail |
| Main's COSM records and Boa default walker integration | Shared helper/scope corrections preserve their supported paths |

### Review exit

- [ ] Map each matrix row to an existing regression or add the missing focused
      case/correction in the implementation review. Identify exact commands and
      live SDK output; do not infer coverage from an aggregate green test count.
- [ ] Refresh the implementation against current main, including the shared
      compiler/COSM changes, then run frontend helper/compiler tests and the
      affected SDK/walker suites.
- [ ] Merge PR #213 (and dependent proof where necessary) before marking Done.
      The new numeric cast helper proposed by xps-8 must extend this shared
      contract rather than bypass it.

## Dispatch for review closure

Inspect `crush-frontend/src/{walker_intrinsics,semantics,optimizer,compiler}`,
`tests/walker_intrinsics.rs`, SDK source tests and xps-6/7 binding/value checks.
Use the existing implementation branch or a focused dependent correction
worktree. Validate one missing row at a time; preserve all previous proof and
leave unrelated language changes with their owning tickets.
