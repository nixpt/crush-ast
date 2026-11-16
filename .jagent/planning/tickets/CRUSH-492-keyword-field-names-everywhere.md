# CRUSH-492 — Allow keyword field names in object literals, struct declarations and struct literals

| Field | Value |
|-------|-------|
| **ID** | CRUSH-492 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#492](https://github.com/nixpt/crush-ast/issues/492) |
| **Size** | M |
| **Dependencies** | CRUSH-455, CRUSH-475, CRUSH-491, CRUSH-453 |

## Problem

Keyword field names are accepted only after `.` (parser/mod.rs:1908-1925, CRUSH-367). Object-literal keys (parse_object_literal :2350, Ident or String only), struct declaration fields (parse_struct_def :1314) and struct literals (parse_struct_literal :96-110, Ident only) reject keywords. So `o.match` can be read and written but the only way to create such a field is a string key. Ruling 6a: allow everywhere (#428).

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/parser/mod.rs:96-110, 1314, 1908-1925, 2350-2390
- crates/crush-frontend/src/parser/lexer.rs:165-200
- issue #428
- ruling 6a

## Goal

Every word keyword_name accepts is a valid field name in object-literal keys, struct declaration fields and struct-literal fields, as after `.`. Literal keywords true/false/null stay rejected. Shorthand `S { match }` is rejected with a message. The PR decides `and`/`or`/`not` as keys (accepted through a lexer after-key rule, or rejected) and tests it. Localised alias spellings in key position are rejected or kept as written, never silently renamed to English, and that is tested. Output is identical on the scheduler VM, PortableVm and FastVM.

## Non-goals

- tree-sitter grammar (CRUSH-TS-GRAMMAR-FRONTEND-PARITY)
- Making keywords valid variable, function or method-definition names
- Changing member-access behaviour

## Files

- crates/crush-frontend/src/parser/mod.rs
- crates/crush-frontend/tests/compiler_tests.rs
- crates/crush-lang-sdk/tests/tier1_keyword_fields.rs (new)

## Done when

- parser tests: `let o = {match: 1, if: 2}`, `struct S { for: int }`, `S { for: 3 }`, `o.if` all parse; `S { match }` and `{true: 1}` are rejected
- tests for the `{not: 1}` decision and for a localised spelling as a key (`{设: 1}`): no silent rename to `let`
- engine test: one program building `{match: 1}` and `S { for: 3 }` and printing both fields gives identical output on the scheduler VM, PortableVm and FastVM (one assertion per engine, via the tier1 harness)
- cargo test -p crush-frontend -p crush-lang-sdk passes; cargo +1.100.0 clippy -p crush-frontend -p crush-lang-sdk --all-targets -- -D warnings clean

## Depends on

- CRUSH-455
- CRUSH-475
- CRUSH-491
- CRUSH-453

## Refs

- #428
- ruling 6a

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
