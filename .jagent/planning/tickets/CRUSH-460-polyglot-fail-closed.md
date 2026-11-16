# CRUSH-460 — Fail closed when the sandbox backend would degrade to proot or no isolation

| Field | Value |
|-------|-------|
| **ID** | CRUSH-460 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#460](https://github.com/nixpt/crush-ast/issues/460) |
| **Size** | S |
| **Dependencies** | none |

## Problem

buckets' sandboxed_command uses bwrap when on PATH; otherwise it falls back to proot (no network or PID isolation) and then to a bare Command::new(program) with only a stderr warning (/workspace/projects/buckets/src/sandbox.rs:82-120). crush-vm's build_sandboxed_command calls it (crates/crush-vm/src/bucket_exec.rs:90-136) and never checks which backend was chosen, so a sandboxed-polyglot build on a box without bwrap runs blocks unsandboxed while appearing sandboxed. buckets exposes `pub fn sandbox_available() -> bool` (sandbox.rs:133-136).

## Evidence (at origin/main 7ecd2515)

- /workspace/projects/buckets/src/sandbox.rs:82-120, 133-136
- crates/crush-vm/src/bucket_exec.rs:90-136
- crates/crush-vm/src/scheduler.rs:~400-405 (sandbox_setup_error mapping)

## Goal

At the top of build_sandboxed_command, before validate/resolve, crush-vm checks that genuine isolation (bwrap today) is available; if not it returns an error mapped through sandbox_setup_error with an install hint. The check sits behind one small backend-neutral crush-vm function (e.g. isolation_available) wrapping buckets::sandbox::sandbox_available, so a zandbox backend replaces one function. It takes its PATH as a parameter (or has an inner fn that does) so tests need no std::env::set_var. No change to the buckets repo.

## Non-goals

- Editing buckets
- Choosing zandbox vs buckets
- Checking that unprivileged user namespaces actually work
- macOS

## Files

- crates/crush-vm/src/bucket_exec.rs
- crates/crush-vm/src/scheduler.rs

## Done when

- unit test (--features sandboxed-polyglot): with an empty PATH dir, build_sandboxed_command returns Err containing an install hint and buckets resolution is never reached (offline, sub-second)
- unit test: a PATH dir holding only an executable fake `proot` is still refused
- unit test: a PATH dir with a fake `bwrap` passes the check
- cargo test -p crush-vm --features sandboxed-polyglot --lib bucket_exec passes without network
- cargo +1.100.0 clippy -p crush-vm --features sandboxed-polyglot --all-targets -- -D warnings clean

## Depends on

- none

## Refs

- CRUSH-236
- CRUSH-20
- ruling 22

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
