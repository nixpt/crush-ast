# CRUSH-455 — Merge the two keyword-name helpers and fix `render` round-trip for keyword fields and `new P()(1)`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-455 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#455](https://github.com/nixpt/crush-ast/issues/455) |
| **Size** | M |
| **Dependencies** | none |

## Problem

Token::keyword_name (lexer.rs:165-200) duplicates Parser::kw_text (parser/mod.rs:3689-3720) and the lists already differ: kw_text lacks capability/spawn/yield/export/lang/import and includes And/Or/Not, which it needs for annotation kebab keys such as `rc-refcell-not-send` (call sites :1613, :3460-3464, :3636-3666); keyword_name excludes them on purpose because `&&`/`||`/`!` lex to the same tokens. Separately render.rs:817-820 prints Expression::NewStruct as `P {}`, so `new P()(1)` renders as `P {}(1)`, which reparses differently (#428). crates/crush-cast/src/bin/export-py.rs has no CallValue/MethodCall arm (#428).

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/parser/lexer.rs:165-200
- crates/crush-frontend/src/parser/mod.rs:3689-3720, call sites 1613, 3460-3464, 3636-3666
- crates/crush-frontend/src/render.rs:747-757, 817-820, 821-825
- crates/crush-cast/src/bin/export-py.rs
- issue #428

## Goal

One keyword table, Token::keyword_name. The annotation-context helper becomes a thin wrapper (keyword_name plus And/Or/Not, documented as annotation-only) with no second list. render emits source that reparses to the same CAST for keyword-named fields and for a NewStruct used as a callee. export-py gains CallValue and MethodCall arms, or refuses loudly with a non-zero exit. Lands first among the parser tickets so the reserved-word and keyword-field tickets edit one table.

## Non-goals

- Changing which words are keywords
- Annotation grammar changes beyond keeping today's accepted keys
- tree-sitter `new P()(1)` callee form

## Files

- crates/crush-frontend/src/parser/lexer.rs
- crates/crush-frontend/src/parser/mod.rs
- crates/crush-frontend/src/render.rs
- crates/crush-frontend/tests/render_tests.rs
- crates/crush-frontend/tests/annotation_parse_tests.rs
- crates/crush-cast/src/bin/export-py.rs

## Done when

- `rg -n 'Token::Mut\(_\) => Some' crates/crush-frontend/src` hits only lexer.rs (no second keyword list)
- existing annotation_parse_tests pass unchanged, plus a new test that `rc-refcell-not-send` and `revisit-if` keys still parse
- round-trip tests in render_tests.rs: for `new P()(1)` and `o.match`, parse -> render -> parse yields equal CAST
- export-py on a CallValue program either emits correct Python or exits non-zero with a message (test)
- cargo test -p crush-frontend -p crush-cast passes; cargo +1.100.0 clippy -p crush-frontend -p crush-cast --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- #428

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
