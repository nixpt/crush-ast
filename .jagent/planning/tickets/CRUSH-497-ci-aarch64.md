# CRUSH-497 — Add Linux aarch64 to CI as a Tier-1 platform

| Field | Value |
|-------|-------|
| **ID** | CRUSH-497 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#497](https://github.com/nixpt/crush-ast/issues/497) |
| **Size** | M |
| **Dependencies** | CRUSH-453, CRUSH-494, CRUSH-481 |

## Problem

Every ci.yml job runs on `ubuntu-latest` (x86_64). Ruling 36 makes Linux aarch64 a Tier-1 platform, but no job builds or tests on it. Backlog CRUSH-50 proposes cross/qemu lanes for aarch64+riscv64 and was never started.

## Evidence (at origin/main 7ecd2515)

- .github/workflows/ci.yml runs-on lines (all ubuntu-latest: 17,43,77,116,213,292,324,378,426,468,493,526,577,668,697)
- .jagent/planning/tickets/CRUSH-50-ci-multi-arch-matrix.md (Backlog)
- ruling 36

## Goal

A CI job on a native arm runner (ubuntu-24.04-arm) with the sibling buckets checkout (as ci.yml:20-25), its own CARGO_TARGET_DIR and rust-cache. It builds and tests the Tier-1 surface: crush-vm (scheduler VM, PortableVm, FastVM), crush-frontend, crush-lang-sdk, casm, crush-cast and tree-sitter-crush, including the tier1_* agreement tests. The JIT is excluded because it is experimental and off by default (H5 checkpoint ruling), not for an architecture reason; GUI/PTX are excluded too. The job becomes a required check once green. It supersedes CRUSH-50's aarch64 half; riscv stays in CRUSH-50.

## Non-goals

- macOS (Tier-2)
- qemu/cross emulation
- riscv64
- Release binaries for aarch64
- Full workspace test suite on arm

## Files

- .github/workflows/ci.yml
- docs/platforms.md
- .jagent/planning/tickets/CRUSH-50-ci-multi-arch-matrix.md

## Done when

- the new job `test-aarch64` is green on the PR run (`gh run view` shows it)
- the job log shows `uname -m` = aarch64 and runs the tier1 agreement tests for the scheduler VM, PortableVm and FastVM
- docs/platforms.md lists Linux x86_64 + aarch64 as Tier-1 and macOS as Tier-2, citing ruling 36
- CRUSH-50 is updated to point at this job for aarch64

## Depends on

- CRUSH-453
- CRUSH-494
- CRUSH-481

## Refs

- CRUSH-50
- CRUSH-22
- ruling 36

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
