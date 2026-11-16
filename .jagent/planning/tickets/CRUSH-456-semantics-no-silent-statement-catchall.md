# CRUSH-456 — Replace the semantic checker's silent `TODO: remaining statements` catch-all with named arms and a loud fallback

| Field | Value |
|-------|-------|
| **ID** | CRUSH-456 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#456](https://github.com/nixpt/crush-ast/issues/456) |
| **Size** | S |
| **Dependencies** | none |

## Problem

semantics.rs:458-460 ends the statement match with `_ => { // TODO: Implement remaining statements }`. Any Statement variant without an arm (Assign, TryCatch, Throw, SetField, FunctionDef, ImplBlock, TraitDef, Break/Continue, Export, Import, the DOM variants, AI) passes with no analysis and no error, and so does any variant added later to crush_cast::Statement (lib.rs:93). Ruling 28 will make Statement #[non_exhaustive], so a wildcard must remain, but it must not be silent.

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/semantics.rs:378-460
- crates/crush-cast/src/lib.rs:93 (pub enum Statement)
- ruling 28

## Goal

The match names every current Statement variant; variants needing no checking get an explicit arm with a one-line reason. The remaining wildcard returns a SemanticError naming the unhandled statement kind. Suite failures this surfaces are fixed if trivial, otherwise listed in the PR as follow-ups.

## Non-goals

- New type or scope checks for variants passed today
- Refactoring the checker structure
- Adding #[non_exhaustive] to Statement (CRUSH-API-NONEXHAUSTIVE-CORE)

## Files

- crates/crush-frontend/src/semantics.rs

## Done when

- every Statement variant in crush-cast/src/lib.rs at the PR's base appears by name in the semantics.rs statement match (list in PR)
- the wildcard arm returns an error whose message contains `not checked` (unit test via a helper that formats the error)
- cargo test -p crush-frontend -p crush-lang-sdk passes; cargo +1.100.0 clippy -p crush-frontend --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- ruling 28

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
