# CRUSH-452 — Delete the crates/crush-aotc crate (checkpoint 1: delete)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-452 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#452](https://github.com/nixpt/crush-ast/issues/452) |
| **Size** | S |
| **Dependencies** | none |

## Problem

The crates/crush-aotc crate (AotcCompiler, NaN-boxed C backend) has no caller, matches crush-run on only 19 of 52 programs (CRUSH-222) and compiles unknown ops to a comment (#434). The captain ruled delete at checkpoint 1 (2026-11-13), which makes #434 and the aotc API work moot. It is still a workspace member and default-member (Cargo.toml:39, :44) and is built in CI (.github/workflows/ci.yml:361). The `crush-aotc` binary of crates/crush-aot (Cargo.toml:45-47, src/bin/aotc.rs) is a different, live tool and is not affected.

## Evidence (at origin/main 7ecd2515)

- crates/crush-aotc/ (Cargo.toml, src, tests)
- Cargo.toml:39 members, :44 default-members
- ​.github/workflows/ci.yml:361 `-p crush-aotc`
- crates/crush-aot/Cargo.toml:45-47 (unrelated crush-aotc binary; keep)
- ​.jagent/planning/tickets/CRUSH-222, CRUSH-144, CRUSH-132, CRUSH-zorro-25
- issue #434; checkpoint 1 ruling on #19

## Goal

crates/crush-aotc and every reference to the crate are removed: workspace members/default-members, Cargo.lock, the CI `-p crush-aotc` line, and docs that describe the crate. The workspace builds. The aotc-crate tickets and #434 are closed as superseded.

## Non-goals

- Changes to crush-aot or its crush-aotc binary
- Salvaging AotcCompiler logic or tests
- Editing historical .dejavue/decisions.md entries

## Files

- crates/crush-aotc/ (delete)
- Cargo.toml
- Cargo.lock
- .github/workflows/ci.yml
- docs/ (crate references only)
- .jagent/planning/tickets/CRUSH-222*, CRUSH-144*, CRUSH-132*, CRUSH-zorro-25*

## Done when

- `rg -n 'crates/crush-aotc|crush_aotc|AotcCompiler|-p crush-aotc' --glob '!.dejavue/**' --glob '!CHANGELOG.md' .` returns no matches
- cargo metadata --format-version 1 lists no package named crush-aotc, and cargo check -p crush-aot --bins still builds crush-aot's crush-aotc binary
- cargo check --workspace passes (debug, one CARGO_TARGET_DIR)
- CRUSH-222, CRUSH-144, CRUSH-132 and CRUSH-zorro-25 are marked Superseded (checkpoint 1); #434 is closed with a link to the PR

## Depends on

- none

## Refs

- CRUSH-222
- CRUSH-144
- CRUSH-132
- CRUSH-zorro-25
- #434
- checkpoint 1 (#19 crush-aotc: delete)

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
