# CRUSH-491 — Mark localised (Nepali/CJK) keyword aliases experimental in code and docs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-491 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#491](https://github.com/nixpt/crush-ast/issues/491) |
| **Size** | S |
| **Dependencies** | CRUSH-475 |

## Problem

read_identifier hard-wires localised aliases into the English keyword match (lexer.rs:626-655: `"let" | "manau" | "设" | "させ"`, likewise fn/if/else/while/return/true/false). Nothing marks them as outside the 1.0 syntax promise. Ruling 8: experimental. Side effect: keyword_name maps an alias back to English, so `o.设` reads the field `let`, undocumented.

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/parser/lexer.rs:155-164, 626-655
- ruling 8

## Goal

The aliases move out of the English keyword match into one named table (e.g. EXPERIMENTAL_KEYWORD_ALIASES) with a doc comment citing ruling 8; read_identifier consults it through one function, so a later opt-in flag is a one-line change. The crush-frontend crate docs state localised keywords are experimental and outside the 1.0 syntax promise. Lexing behaviour is unchanged.

## Non-goals

- A feature flag, a warning, or removing aliases (not ruled)
- Fixing alias-as-field-name behaviour (CRUSH-KEYWORD-FIELD-NAMES-EVERYWHERE decides key position)
- New locales
- Docs site

## Files

- crates/crush-frontend/src/parser/lexer.rs
- crates/crush-frontend/src/lib.rs

## Done when

- `rg -n 'EXPERIMENTAL' crates/crush-frontend/src/parser/lexer.rs crates/crush-frontend/src/lib.rs` finds the alias table doc and the crate-doc line
- table-driven test: every alias accepted before the change still lexes to the same token
- a test records the current behaviour of `o.设` (documented in the test name/comment)
- cargo test -p crush-frontend passes; cargo +1.100.0 clippy -p crush-frontend --all-targets -- -D warnings clean

## Depends on

- CRUSH-475

## Refs

- ruling 8

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
