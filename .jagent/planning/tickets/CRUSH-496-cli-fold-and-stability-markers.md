# CRUSH-496 — Fold the remaining stable binaries into `crush <sub>` and mark debug/gui and the aux binaries experimental (ruling 35)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-496 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#496](https://github.com/nixpt/crush-ast/issues/496) |
| **Size** | M |
| **Dependencies** | CRUSH-452, CRUSH-470, CRUSH-495 |

## Problem

Ruling 35: fold the stable entry points into `crush <sub>`; the remaining aux binaries are unstable; LSP and debugger are experimental. The umbrella `crush` binary (crates/crush-lang-sdk/src/bin/crush.rs:138-176) routes repl/run/build/cast/debug/gui/doctor/update by exec-ing sibling binaries, but two shipped binaries have no route: crush-compile, the .cvms/.casm -> .cvm1 assembler (crush-compile.rs:1-15), and crush-diff, a cross-backend differential runner that exits 1 on divergence (crush-diff.rs:1-6). `crush cast diff` already exists (cast_cli.rs:67) and means CAST diff, so `crush diff` would be ambiguous. Meanwhile the umbrella presents `debug` and `gui` as ordinary subcommands (crush.rs:127-130, 156-163), and the aux binaries crush-walk-run, crush-aot-runner and jit-runner (crates/crush-aot/Cargo.toml:46-58) carry no stability label.

## Evidence (at origin/main 7ecd2515)

- crates/crush-lang-sdk/src/bin/crush.rs:120-176
- crates/crush-lang-sdk/src/bin/crush-compile.rs:1-30
- crates/crush-lang-sdk/src/bin/crush-diff.rs:1-25
- crates/crush-lang-sdk/src/cast_cli.rs:67
- crates/crush-lang-sdk/src/bin/crush-debug.rs
- crates/crush-aot/Cargo.toml:46-58; crates/crush-aot/src/bin/{walk_run,aot_runner}.rs
- rulings 35, 34

## Goal

(a) `crush asm` (or `crush assemble`; record the choice) runs the crush-compile logic in-process through a shared library fn. The differential runner is either exposed as `crush check-engines` (an unambiguous name, not `diff`) or classified aux/unstable; record which. Old binary names stay as shims printing a one-line deprecation note to stderr (ruling 34). `crush --help` lists the new subcommands. (b) `crush --help` tags `debug` and `gui` (experimental); running them prints a one-line stderr notice suppressible by CRUSH_NO_UNSTABLE_NOTICE=1. Each aux binary's --help starts with 'UNSTABLE:'. docs/cli-stability.md lists every shipped binary as stable or experimental/unstable.

## Non-goals

- `crush fix` (not ticketed; see dropped)
- Removing or renaming binaries
- Converting existing exec-based routes to in-process
- LSP changes beyond the label
- Crate-level labels (CRUSH-LOCKSTEP-VERSION-AND-STABILITY-LABELS)

## Files

- crates/crush-lang-sdk/src/bin/crush.rs
- crates/crush-lang-sdk/src/bin/crush-compile.rs
- crates/crush-lang-sdk/src/bin/crush-diff.rs
- crates/crush-lang-sdk/src/bin/crush-debug.rs
- crates/crush-lang-sdk/src/lib.rs
- crates/crush-aot/src/bin/walk_run.rs
- crates/crush-aot/src/bin/aot_runner.rs
- crates/crush-jit/src/bin/jit_runner.rs
- crates/crush-gui/src/main.rs
- docs/cli-stability.md
- crates/crush-lang-sdk/tests/cli_fold.rs (new)
- crates/crush-lang-sdk/tests/cli_stability.rs (new)

## Done when

- cli_fold: `crush asm f.cvms -o a.cvm1` and `crush-compile f.cvms -o b.cvm1` produce byte-identical .cvm1 files, and `crush-compile` prints a one-line deprecation note on stderr
- cli_fold: the folded differential subcommand (if folded) and `crush-diff` give the same exit code and stdout on one agreeing and one diverging fixture
- cli_stability: `crush --help` lists the new subcommand(s) and tags exactly `debug` and `gui` as experimental; `crush debug --help` stdout and exit code are unchanged, the notice appears on stderr and is absent with CRUSH_NO_UNSTABLE_NOTICE=1
- a test asserts `crush-walk-run --help` output starts with UNSTABLE
- cargo test -p crush-lang-sdk --test cli_fold --test cli_stability and cargo test -p crush-aot pass (debug builds only)
- cargo +1.100.0 clippy -p crush-lang-sdk -p crush-aot --all-targets -- -D warnings is clean

## Depends on

- CRUSH-452
- CRUSH-470
- CRUSH-495

## Refs

- ruling 35
- ruling 34

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
