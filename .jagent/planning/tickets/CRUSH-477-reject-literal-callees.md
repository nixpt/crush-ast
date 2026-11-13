# CRUSH-477 — Reject literal callees (`5(3)`, `"s"(1)`, `{a:1}(2)`) statically

| Field | Value |
|-------|-------|
| **ID** | CRUSH-477 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#477](https://github.com/nixpt/crush-ast/issues/477) |
| **Size** | S |
| **Dependencies** | CRUSH-456 |

## Problem

parser/mod.rs:2071-2080 turns any non-name, non-member postfix callee into Expression::CallValue and compiler.rs:2549-2566 emits `call_value` for it, so `5(3)` compiles and fails only at run time with NotCallable. Before CRUSH-363 these failed at compile time. Ruling 6b: reject statically (#428).

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/parser/mod.rs:2071-2080
- crates/crush-frontend/src/compiler.rs:2549-2566
- issue #428
- ruling 6b

## Goal

A CallValue whose callee is a literal (number, string, bool, null, array literal, object literal; the PR lists the exact Expression variants) produces a located compile error `cannot call a <kind> literal`. The check runs in the compile or semantic path so it covers both source and CAST JSON input. Closures, calls, index, member and name expressions stay callable; runtime NotCallable for computed callees is unchanged.

## Non-goals

- Static type-based callability inference
- Keyword field names
- `render` round-trip of `new P()(1)`

## Files

- crates/crush-frontend/src/compiler.rs
- crates/crush-frontend/src/semantics.rs (if the check lives there)
- crates/crush-frontend/tests/compiler_tests.rs

## Done when

- tests: `5(3)`, `"s"(1)`, `{a: 1}(2)` (expression position), `[1](0)` each fail to compile with a located error containing `cannot call`
- test: a hand-built CAST CallValue with a Literal callee is refused the same way
- tests: `h[k](41)`, `f(x)(y)`, `(|| 1)()` still compile
- no engine test needed (the error is raised before CASM); stated in the PR
- cargo test -p crush-frontend passes; cargo +1.100.0 clippy -p crush-frontend --all-targets -- -D warnings clean

## Depends on

- CRUSH-456

## Refs

- #428
- ruling 6b

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
