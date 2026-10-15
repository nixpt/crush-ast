# CRUSH-xps-10 — Validate walker intrinsics and loop scopes in semantic analysis

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-10 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 / SDK correctness |
| **Assignee** | unassigned |
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

- [ ] Compiler-owned walker intrinsics are recognized by semantic analysis
      through a shared or explicitly synchronized contract; arbitrary unknown
      function calls remain errors.
- [ ] Intrinsic arguments and arity/target shape are checked without treating
      compiler intrinsics as host capabilities.
- [ ] `Statement::For` checks its iterable, binds its iteration variable in a
      loop-local scope, checks the body, and does not leak that binding after
      the loop.
- [ ] SDK Rust `while` assignment and `for` range accumulation execute with
      exact expected output (`5` and `45`).
- [ ] Any other enabled walker using `__crush_assign__` has a focused
      regression test through its supported compile path.

## Notes

- Found by the failing `Test (sdk)` jobs on [PR #213](https://github.com/nixpt/crush-ast/pull/213).
- The compiler already lowers `__crush_assign__` as an intrinsic; the defect
  is the earlier semantic-analysis gate.
