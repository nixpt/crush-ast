# CRUSH-476 — Retire `use @mcp` and `import @http/@git/@file` as parser forms; error says reserved

| Field | Value |
|-------|-------|
| **ID** | CRUSH-476 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#476](https://github.com/nixpt/crush-ast/issues/476) |
| **Size** | M |
| **Dependencies** | CRUSH-475 |

## Problem

The parser still builds AST nodes for `use @mcp` (parser/mod.rs:2649 -> parse_mcp_import :2777) and external imports (:2679 -> parse_external_import :3033). They are refused only later by import_system::unsupported_import (import_system.rs:171-202) with `... is not supported yet`. 'Yet' promises a feature; ruling 7 says these forms are reserved with no 1.0 form.

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/parser/mod.rs:2649,2679,2777,3033
- crates/crush-frontend/src/import_system.rs:171-202
- crates/crush-frontend/tests/compiler_tests.rs:478,486 and crates/crush-lang-sdk/tests/imports_cli.rs:446 assert the old wording
- ruling 7

## Goal

From source, `use @mcp ...` and `import @http|@git|@file ...` are rejected at parse time with a located message saying the form is reserved and has no 1.0 form. For CAST input already holding ImportStatement::External/MCPImport (agent-emitted JSON), unsupported_import still refuses at compile time with the same reserved wording. `use @cap`, `use @lang` and secure-env imports are untouched.

## Non-goals

- Removing the ImportStatement::MCPImport/External variants from crush-cast
- Removing the compile-time refusal for CAST input
- Implementing MCP or external loading
- tree-sitter test_imports.crush (CRUSH-TS-GRAMMAR-FRONTEND-PARITY)

## Files

- crates/crush-frontend/src/parser/mod.rs
- crates/crush-frontend/src/import_system.rs
- crates/crush-frontend/tests/compiler_tests.rs
- crates/crush-lang-sdk/tests/imports_cli.rs

## Done when

- tests: source `use @mcp "x" { "t" } as m` and `import @http "u" as d` each fail at parse time with a message containing `reserved`
- test: a CAST program built with ImportStatement::External / MCPImport still fails to compile, message contains `reserved`
- `rg -n 'not supported yet' crates/crush-frontend/src/import_system.rs` returns nothing for @mcp/@http/@git/@file
- cargo test -p crush-frontend and cargo test -p crush-lang-sdk --test imports_cli pass; cargo +1.100.0 clippy -p crush-frontend --all-targets -- -D warnings clean

## Depends on

- CRUSH-475

## Refs

- ruling 7

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
