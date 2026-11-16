# CRUSH-481 — MSRV policy N-2 stable, derived from one source and checked in CI

| Field | Value |
|-------|-------|
| **ID** | CRUSH-481 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#481](https://github.com/nixpt/crush-ast/issues/481) |
| **Size** | S |
| **Dependencies** | none |

## Problem

The MSRV is hand-pinned to 1.95.0 in many places: Cargo.toml:70 (workspace), the ci.yml msrv job name and RUSTUP_TOOLCHAIN (ci.yml:42-46, 'keep in step' comment, CRUSH-231), crush-gui/Cargo.toml:9, crush-debugger/Cargo.toml:9, crush-vm-py/Cargo.toml:20 and crush-web/Cargo.toml:17 (both excluded from the workspace, so literals are required), and README.md:16 (badge) and :134. Ruling 29 sets N-2 stable and makes a bump a minor release, but there is no written policy and nothing checks the values agree.

## Evidence (at origin/main 7ecd2515)

- Cargo.toml:62, :70
- crates/crush-vm-py/Cargo.toml:15-20
- crates/crush-web/Cargo.toml:17
- crates/crush-gui/Cargo.toml:9; crates/crush-debugger/Cargo.toml:9
- .github/workflows/ci.yml:36-46
- README.md:16,134
- ruling 29

## Goal

docs/msrv-policy.md states N-2 stable and that a bump is a minor release. Workspace members (crush-gui, crush-debugger) use `rust-version.workspace = true`. The excluded crates keep literals that a script verifies against Cargo.toml, together with ci.yml's msrv toolchain and job name and the README badge and text. The MSRV value is set per the policy on the day the ticket lands, recorded in the PR. Clippy stays pinned to 1.100.0.

## Non-goals

- Changing the clippy pin (#438)
- Dropping edition 2024
- Per-crate MSRVs
- Bringing crush-web/crush-vm-py into the workspace

## Files

- Cargo.toml
- crates/crush-gui/Cargo.toml
- crates/crush-debugger/Cargo.toml
- crates/crush-vm-py/Cargo.toml
- crates/crush-web/Cargo.toml
- .github/workflows/ci.yml
- README.md
- docs/msrv-policy.md
- scripts/check-msrv-sync.sh

## Done when

- `scripts/check-msrv-sync.sh` exits 0, and exits 1 when any of ci.yml, crush-vm-py, crush-web or README disagrees with Cargo.toml rust-version (shown with a temp edit)
- `grep -n 'rust-version' crates/crush-gui/Cargo.toml crates/crush-debugger/Cargo.toml` shows only `.workspace = true`
- the ci.yml msrv job runs check-msrv-sync.sh and is green on the PR
- cargo +1.100.0 clippy --workspace -- -D warnings is still clean

## Depends on

- none

## Refs

- CRUSH-231
- ruling 29
- #438

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
