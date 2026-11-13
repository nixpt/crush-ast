# CRUSH-482 — Lock-step 1.0: every crate and Python package carries the workspace version, and experimental crates are labelled

| Field | Value |
|-------|-------|
| **ID** | CRUSH-482 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#482](https://github.com/nixpt/crush-ast/issues/482) |
| **Size** | M |
| **Dependencies** | CRUSH-452 |

## Problem

Ruling 27 with checkpoint 1: lock-step versioning to 1.0, experimental crates labelled, no per-crate tooling. Neither half holds. (a) Version drift: crush-vm-py hard-codes 0.3.0 in Cargo.toml:18 and pyproject.toml:7 (it is excluded from the workspace, Cargo.toml:62, so it cannot use version.workspace), crush-python's pyproject.toml:7 hard-codes 0.2.0, the workspace is at 0.11.2 and scripts/bump-version.sh touches none of these (#394, #395). (b) Labels: crates/ holds 45 crates all shipping at one version and nothing tells consumers which are stable; no `stability` key exists in any crates/*/Cargo.toml. Ruling 35 makes the LSP and debugger experimental.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm-py/Cargo.toml:15-18; crates/crush-vm-py/pyproject.toml:7
- crates/crush-python/pyproject.toml:7
- Cargo.toml:62 (exclude), :65 (version 0.11.2)
- scripts/bump-version.sh
- issues #394, #395
- `ls crates | wc -l` -> 45; no stability key in any crates/*/Cargo.toml
- ruling 27 + checkpoint 1 versioning ruling; ruling 35

## Goal

(a) pyproject.toml files use `dynamic = ["version"]` so maturin reads Cargo.toml, or bump-version.sh rewrites them (record which); bump-version.sh also rewrites the excluded crates' literal Cargo.toml versions; scripts/check-versions.sh fails CI when any workspace crate, excluded crate or pyproject version differs from the workspace version. Closes #394 and #395. (b) Each crate gets `[package.metadata.crush] stability = "stable"|"experimental"`; every experimental crate's description starts with `[experimental]` and its README/lib.rs carries a banner; docs/stability.md lists every crate once with the ruling-27 area that justifies it. Stable candidates: crush-cast, casm, crush-cosm, crush-errors, crush-caison, crush-frontend, crush-vm, crush-lang-sdk. Experimental: walkers, polyglot, AI ops, GPU/ptx, jit, debugger, gui, tui, LSP. Crates ruling 27 does not settle (notably crush-aot, crush-pkg/crush-installer) are listed as captain questions in the PR, not decided by the ticket.

## Non-goals

- Per-crate versioning
- Publishing wheels or wheel CI (CRUSH-285)
- Bringing excluded crates into the workspace
- Deciding open items (H3, H4, #2, #31)
- Code changes beyond doc banners
- Binary/subcommand labels (CRUSH-CLI-FOLD-AND-STABILITY-MARKERS)

## Files

- crates/*/Cargo.toml
- crates/*/src/lib.rs (banners for experimental crates)
- crates/crush-vm-py/pyproject.toml
- crates/crush-python/pyproject.toml
- scripts/bump-version.sh
- scripts/check-versions.sh
- scripts/check-stability-labels.sh
- docs/stability.md
- README.md
- .github/workflows/ci.yml

## Done when

- `scripts/check-versions.sh` exits 0 on the fixed tree and exits 1 with crush-vm-py's old 0.3.0 restored; `cargo metadata --no-deps --manifest-path crates/crush-vm-py/Cargo.toml --format-version 1` reports the workspace version
- running bump-version.sh's version-rewrite function on a temp copy with a new version leaves check-versions.sh green
- `scripts/check-stability-labels.sh` exits 0 (every crate under crates/ has stability = stable|experimental, every experimental description starts with [experimental], docs/stability.md lists every crates/ directory exactly once) and exits 1 when one crate's key is removed
- both scripts run in a CI job, green on the PR
- the PR lists the crates left as captain questions (at least crush-aot)

## Depends on

- CRUSH-452

## Refs

- #394
- #395
- CRUSH-285
- ruling 27
- ruling 35
- checkpoint 1 versioning ruling

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
