# CRUSH-459 — Polyglot is sandboxed by default; unsandboxed needs an explicit --polyglot-unsandboxed flag

| Field | Value |
|-------|-------|
| **ID** | CRUSH-459 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#459](https://github.com/nixpt/crush-ast/issues/459) |
| **Size** | M |
| **Dependencies** | none |

## Problem

A granted @python/@javascript/@bash block runs `Command::new(binary)` with the host's full authority unless crush-vm is built with the opt-in `sandboxed-polyglot` feature (crates/crush-vm/Cargo.toml:89; unsandboxed branch at scheduler.rs:407-419). crush-lang-sdk does not forward that feature and its default list (Cargo.toml:43) lacks it, so the shipped CLI can never be sandboxed. `--polyglot` grants python/javascript/bash (crush-run.rs:95-98, 708-712) and its help says the blocks run 'with the host process's authority'; the same grant exists in crush-debug (crush-debug.rs:87,202) and crush-gui (run.rs:33,128). The scheduler VM and PortableVm both execute blocks through the default SubprocessExecutor -> scheduler::run_exec_lang_stdout (lang_exec.rs:174-189; portable_vm.rs:2047-2060), the one enforcement point. Ruling 22 requires sandboxed by default with an explicit unsandboxed flag (CRUSH-236 criterion 1).

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/Cargo.toml:89
- crates/crush-vm/src/scheduler.rs:407-419
- crates/crush-vm/src/lang_exec.rs:174-189
- crates/crush-lang-sdk/Cargo.toml:43
- crates/crush-lang-sdk/src/bin/crush-run.rs:95-98,708-712
- crates/crush-lang-sdk/src/bin/crush-debug.rs:87,202; crates/crush-gui/src/run.rs:33,128
- crates/crush-lang-sdk/src/effects.rs:137
- ruling 22; CRUSH-236 criterion 1

## Goal

(1) crush-vm: the built-in SubprocessExecutor refuses to spawn unsandboxed unless the host set one explicit opt-in on HostCaps/HostCapsBuilder (e.g. allow_unsandboxed_polyglot); without it a build lacking the sandbox returns a VmError naming --polyglot-unsandboxed. (2) crush-lang-sdk gets a default-on `sandboxed-polyglot = ["crush-vm/sandboxed-polyglot"]` feature. (3) crush-run, crush-debug and crush-gui expose `--polyglot` (sandboxed) and `--polyglot-unsandboxed` (grants plus the opt-in; help names the host authority). (4) The effects.rs grant hint mentions both flags. Flag names and refusal text are backend-neutral (no 'buckets' or 'bwrap' in the CLI surface), so a zandbox swap changes nothing user-facing. Where no sandbox backend exists (macOS, no bwrap) only --polyglot-unsandboxed runs blocks; the docs say so.

## Non-goals

- Choosing zandbox vs buckets (altair's note)
- Refusing when the backend degrades to proot or nothing (CRUSH-POLYGLOT-FAIL-CLOSED)
- The loud error for dropped pinned deps (CRUSH-POLYGLOT-PINNED-DEPS-LOUD)
- Making macOS sandboxed
- Embedder-supplied PolyglotExecutors

## Files

- crates/crush-vm/src/scheduler.rs
- crates/crush-vm/src/lang_exec.rs
- crates/crush-vm/src/host.rs
- crates/crush-lang-sdk/Cargo.toml
- crates/crush-lang-sdk/src/bin/crush-run.rs
- crates/crush-lang-sdk/src/bin/crush-debug.rs
- crates/crush-lang-sdk/src/effects.rs
- crates/crush-gui/src/run.rs
- crates/crush-gui/src/bin/crush-gui.rs

## Done when

- crush-vm unit test, built without sandboxed-polyglot: a granted @bash block on the scheduler VM without the opt-in returns Err containing '--polyglot-unsandboxed' and no process is spawned; the same test on PortableVm
- with the opt-in set, the same block returns its stdout on the scheduler VM and on PortableVm
- crush-lang-sdk integration test: `crush-run run --polyglot-unsandboxed` on a @bash echo block prints the output and exits 0; `--polyglot` on the same block either runs sandboxed (bwrap present) or exits non-zero naming --polyglot-unsandboxed, never running unsandboxed
- `crush-run run --help`, `crush-debug --help` and `crush-gui --help` list both flags
- cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk -p crush-gui --all-targets -- -D warnings is clean, with and without crush-vm/sandboxed-polyglot

## Depends on

- none

## Refs

- CRUSH-236
- CRUSH-20
- CRUSH-2
- CRUSH-271
- ruling 22

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
