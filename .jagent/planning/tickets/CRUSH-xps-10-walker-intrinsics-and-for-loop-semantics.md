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
