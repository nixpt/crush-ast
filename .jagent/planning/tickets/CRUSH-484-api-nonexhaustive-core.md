# CRUSH-484 — #[non_exhaustive] on growable public enums: cast, cosm, casm, errors, frontend

| Field | Value |
|-------|-------|
| **ID** | CRUSH-484 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#484](https://github.com/nixpt/crush-ast/issues/484) |
| **Size** | L |
| **Dependencies** | CRUSH-456, CRUSH-462, CRUSH-458 |

## Problem

Ruling 28 makes #[non_exhaustive] mandatory on growable public enums. The workspace has about 150 `pub enum` declarations and 3 #[non_exhaustive] attributes (crush-errors/src/version.rs:25, crush-vm/src/lang_exec.rs:47,83). In this ticket's crates: crush-cast 29, crush-cosm 17 (incl. CosmError lib.rs:52), casm 4, crush-errors 3 (1 annotated), crush-frontend 8: 61 enums with no classification.

## Evidence (at origin/main 7ecd2515)

- `grep -rn '^\s*pub enum' crates/<crate>/src` -> cast 29, cosm 17, casm 4, errors 3, frontend 8
- `grep -rn non_exhaustive crates --include='*.rs'` -> 3 attributes
- crates/crush-cosm/src/lib.rs:52
- crates/crush-errors/src/version.rs:25

## Goal

Each pub enum in the five crates is classified. A growable enum gets #[non_exhaustive]. A closed enum goes in an allowlist (docs/api-stability.md or a data file) with a one-line reason; this covers wire-format tags frozen by the CAST/CASM format version and Tier-1 engine dispatch enums where exhaustiveness checking protects parity. Every wildcard arm added in another crate because of this change refuses loudly (error or unsupported-op diagnostic) and never silently defaults. A script enforces the classification.

## Non-goals

- crush-vm and crush-lang-sdk enums (CRUSH-API-NONEXHAUSTIVE-VM)
- Experimental crates
- Changing serde representations or wire formats

## Files

- crates/crush-cast/src
- crates/crush-cosm/src
- crates/casm/src
- crates/crush-errors/src
- crates/crush-frontend/src
- scripts/check-non-exhaustive.sh
- docs/api-stability.md
- downstream match sites (wildcard arms only)

## Done when

- `scripts/check-non-exhaustive.sh crush-cast crush-cosm casm crush-errors crush-frontend` exits 0, and exits 1 when a temporary unannotated, unallowlisted pub enum is added (shown in the PR)
- every new `_ =>` arm added outside the defining crate returns an error or refusal; the list of sites is in the PR description
- cargo check --workspace --all-targets is green
- cargo test -p crush-cast -p crush-cosm -p casm -p crush-frontend passes
- cargo +1.100.0 clippy -p crush-cast -p crush-cosm -p casm -p crush-errors -p crush-frontend --all-targets -- -D warnings is clean

## Depends on

- CRUSH-456
- CRUSH-462
- CRUSH-458

## Refs

- ruling 28
- CRUSH-30

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
