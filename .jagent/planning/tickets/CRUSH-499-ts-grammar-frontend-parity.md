# CRUSH-499 — Bring tree-sitter-crush in line with the 1.0 surface (keyword fields, reserved forms)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-499 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#499](https://github.com/nixpt/crush-ast/issues/499) |
| **Size** | M |
| **Dependencies** | CRUSH-492, CRUSH-475, CRUSH-476 |

## Problem

grammar.js defines identifier as /[a-zA-Z_]\w*/ (:526); struct_field and lambda_parameter names are `identifier` (:130-146) and object_property keys are string|identifier (:544), so keyword field names (ruling 6a) cannot parse. The grammar still accepts forms ruling 7 reserves: capability_decl (:390-397), optional('pub') in fn_def (:108), and the corpus/fixtures use them (test/corpus/modern.txt:246; test_imports.crush:19-29 for @mcp/@git/@http). tree-sitter-crush 0.3.0 is published and feeds editors and crush-lsp, so the grammar and the frontend drift silently (CRUSH-207).

## Evidence (at origin/main 7ecd2515)

- crates/tree-sitter-crush/grammar.js:108, 130-146, 390-397, 526, 537-547
- crates/tree-sitter-crush/test/corpus/modern.txt:246
- crates/tree-sitter-crush/test_imports.crush:19-29
- crates/tree-sitter-crush/tests/grammar_examples.rs (CI runs it; CLI corpus is not run: .github/workflows/ci.yml:317)
- .jagent/planning/tickets/CRUSH-207-tree-sitter-grammar-drift.md

## Goal

The grammar accepts keyword field names exactly where the frontend does after CRUSH-KEYWORD-FIELD-NAMES-EVERYWHERE: member access, object keys, struct fields and struct literals. It drops capability_decl, the `pub` prefix and the @mcp/@http/@git/@file import forms the frontend now rejects. The corpus and fixtures are updated and src/ is regenerated.

## Non-goals

- Publishing to crates.io or a version bump (lock-step release)
- `new P()(1)` callee form
- Changing polydex or crush-lsp code
- The CRUSH-207 differential test

## Files

- crates/tree-sitter-crush/grammar.js
- crates/tree-sitter-crush/test/corpus/
- crates/tree-sitter-crush/test_imports.crush
- crates/tree-sitter-crush/queries/
- crates/tree-sitter-crush/src/ (regenerated parser)

## Done when

- cargo test -p tree-sitter-crush passes (grammar_examples over examples/crush, KNOWN_BAD list not grown)
- corpus cases for `{match: 1}`, `struct S { for: int }`, `S { for: 3 }`, `o.if` added; `npx tree-sitter test` in crates/tree-sitter-crush passes (run locally, result pasted in PR)
- `rg -n 'capability_decl|optional\(.pub.\)' crates/tree-sitter-crush/grammar.js crates/tree-sitter-crush/queries` returns nothing
- the PR lists removed node names and whether `rg` finds them in polydex / crush-lsp queries (no builds of those repos)

## Depends on

- CRUSH-492
- CRUSH-475
- CRUSH-476

## Refs

- CRUSH-207
- #428
- rulings 6a, 7

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
