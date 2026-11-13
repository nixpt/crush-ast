# CRUSH-462 — crush-frontend: replace anyhow in the public API with its own error type

| Field | Value |
|-------|-------|
| **ID** | CRUSH-462 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#462](https://github.com/nixpt/crush-ast/issues/462) |
| **Size** | L |
| **Dependencies** | none |

## Problem

crush-frontend exposes anyhow in its public API: compiler.rs:46 `pub type CompileError = anyhow::Error`; lib.rs:19 `use anyhow::Result` makes every pub fn there return anyhow (parse_source :94, parse_file :104, parse_source_with_imports :118, compile_cast :161, compile_cast_owned :169, compile_cast_with_cosm :183, compile_crush_source :215, lower_to_cosm :225, list_symbols :266, extract_symbol :304, extract_manifest :315); lib.rs:53 `pub fn semantic_error(err: anyhow::Error) -> anyhow::Error`. anyhow appears in 7 src files. Ruling 28: stable crates expose their own Error types. About 20 workspace crates call crush_frontend:: and need adapting.

## Evidence (at origin/main 7ecd2515)

- crates/crush-frontend/src/compiler.rs:46
- crates/crush-frontend/src/lib.rs:19,53,94,104,161,215,266,304,315
- crates/crush-frontend/Cargo.toml (anyhow.workspace)
- `grep -rln 'crush_frontend::' crates` -> 20 crates
- ruling 28

## Goal

Every pub fn and pub type of crush-frontend uses a crate-owned FrontendError (#[non_exhaustive], std::error::Error + Display, message text unchanged). CompileError becomes a real type or an alias of FrontendError. The SemanticError case is a variant or typed accessor, not an anyhow downcast. anyhow may stay as a private detail but never in a public signature. Downstream crates are adapted minimally (`?` through From impls or `.map_err`).

## Non-goals

- crush-lang-sdk's own error type (CRUSH-API-ERR-SDK)
- Changing diagnostic wording or the JSON diagnostics shape
- Error types for experimental crates (walkers, jit, aot)

## Files

- crates/crush-frontend/src/lib.rs
- crates/crush-frontend/src/compiler.rs
- crates/crush-frontend/src/error.rs
- crates/crush-frontend/src/parser/
- crates/crush-frontend/Cargo.toml
- crates/crush-frontend/tests/public_error_type.rs
- call sites in downstream crates (minimal)

## Done when

- `grep -rn 'anyhow' crates/crush-frontend/src/lib.rs` and `grep -rn 'use anyhow::Result' crates/crush-frontend/src` print nothing
- tests/public_error_type.rs compiles functions with explicit `Result<_, crush_frontend::FrontendError>` around parse_source, parse_file, compile_cast, compile_crush_source and list_symbols; it asserts parse_source on invalid source returns Err usable as `&dyn std::error::Error`, and that a semantic error is distinguishable without anyhow downcasting
- cargo test -p crush-frontend passes with existing diagnostic message assertions unchanged
- cargo check --workspace --all-targets is green
- cargo +1.100.0 clippy -p crush-frontend --all-targets -- -D warnings is clean

## Depends on

- none

## Refs

- ruling 28

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
