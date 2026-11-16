# CRUSH-483 — crush-lang-sdk: own error type for the compile/run public API

| Field | Value |
|-------|-------|
| **ID** | CRUSH-483 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#483](https://github.com/nixpt/crush-ast/issues/483) |
| **Size** | M |
| **Dependencies** | CRUSH-462 |

## Problem

crush-lang-sdk's public compile entry points return anyhow::Result: compile_crush_source_for_debug (compile.rs:37-39), compile_crush_source (:46), compile_crush_source_with (:53-56), compile_source (:70-73, anyhow::bail! at :96), compile_rust_source (:101), compile_crush_to_casm (:126). anyhow appears in 8 non-bin src files. runtime.rs already has a crate-owned RuntimeError (runtime.rs:60), so the SDK is half-converted. Ruling 28: stable crates expose their own Error types.

## Evidence (at origin/main 7ecd2515)

- crates/crush-lang-sdk/src/compile.rs:37-39,46,53-56,70-73,96,101,126
- crates/crush-lang-sdk/src/runtime.rs:60
- crates/crush-lang-sdk/Cargo.toml:116
- `grep -rln anyhow crates/crush-lang-sdk/src | grep -v /bin/` -> 8 files
- ruling 28

## Goal

Every pub fn outside src/bin/ in crush-lang-sdk returns a crate-owned error: either a new SdkError (#[non_exhaustive], wrapping FrontendError, walker errors as boxed std errors, and RuntimeError) or an extended RuntimeError, chosen and recorded. Display text is unchanged. Binaries in src/bin/ may keep anyhow.

## Non-goals

- Experimental walker crates
- Debugger/REPL internals beyond their pub signatures
- Changing CLI error text

## Files

- crates/crush-lang-sdk/src/compile.rs
- crates/crush-lang-sdk/src/runtime.rs
- crates/crush-lang-sdk/src/error.rs
- crates/crush-lang-sdk/src/lib.rs
- the other non-bin src files that use anyhow
- crates/crush-lang-sdk/src/bin/*.rs (call-site adaptation only)

## Done when

- `grep -rn 'anyhow' crates/crush-lang-sdk/src --include='*.rs' | grep -v '/bin/' | grep -E 'pub fn|-> anyhow|use anyhow::Result'` prints nothing
- a test calls compile_crush_source on invalid source with an explicit `Result<_, crush_lang_sdk::SdkError>` (or the chosen type) and asserts its Display equals the pre-change message
- cargo test -p crush-lang-sdk passes
- cargo check --workspace --all-targets is green
- cargo +1.100.0 clippy -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-462

## Refs

- ruling 28

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
