# CRUSH-167 — Fold squeeze's build-then-run flow into crush-pkg (decision-gated)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-167 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M0 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-161, decision C-4 |
| **Estimated effort** | S (~40 turns) |
| **Relay lane** | D2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

squeeze (`nixpt/squeeze`, 227 L, unpublished, no tests) is ~60 lines of composition over crush-pkg: default check → build → run, and a guard refusing non-Crush capsules for `build`/`check`. It cannot ship before crush-pkg, and SQUEEZE-5 would re-wrap commands crush-pkg already has.

## Success criteria

- [ ] decision C-4 recorded
- [ ] if fold: `crush-pkg` gains the default build-then-run command (with args pass-through and the existing `--message-format`) and the non-Crush guard; tests for both
- [ ] if keep-thin: squeeze becomes a binary that calls into crush-pkg's CLI entry point (crush-pkg exposes one), its dead `crush-vm` dep removed (SQUEEZE-7), requirement raised to `0.3.9`
- [ ] squeeze's README states the outcome (separate PR in `nixpt/squeeze`, owner's call)

## Technical approach

- Keep crush-pkg's existing subcommands unchanged.

## Files to modify

- `crates/crush-pkg/src/main.rs` (or `cli.rs`)
- `crates/crush-pkg/tests/`

## Non-goals

- squeeze's M3–M6 roadmap

## Source (reference only — re-implement, don't copy)

- `nixpt/squeeze` `src/main.rs`; MIGRATION-INVENTORY §4
