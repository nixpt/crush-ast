# CRUSH-463 — Make HostCap::call_with_deadline mandatory (ruling 23)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-463 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#463](https://github.com/nixpt/crush-ast/issues/463) |
| **Size** | L |
| **Dependencies** | CRUSH-461 |

## Problem

HostCap::call_with_deadline has a default body that discards deadline_ms and calls call() (crates/crush-vm/src/host.rs:70-78), so any capability that can block silently ignores the run's wall-clock budget unless its author overrode the method. About 115 in-tree `impl HostCap for` sites exist outside tests and only about 9 override it (crush-lang-sdk net.rs:93/243, host_caps.rs:1109/1347/1440, effects.rs:379, ai_native/toolchain.rs:64, crush-web clock.rs:92, scheduler.rs:2873). Ruling 23 makes call_with_deadline mandatory and puts wall-clock bounds in the 1.0 promise. It is a breaking trait change for out-of-tree implementers, so it belongs in 0.12 with a CHANGELOG migration note (ruling 34).

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/host.rs:55-78
- rg 'impl.*HostCap for' crates (excluding tests): ~115 sites; rg 'fn call_with_deadline': ~9 overrides
- CRUSH-40-timeout-all-blocking-caps.md (Backlog) residual scope
- rulings 23 and 34

## Goal

call_with_deadline has no default body. Every in-tree HostCap states its deadline handling: pure or non-blocking caps delegate explicitly (a small helper or macro is fine), blocking caps (net/*, proc/*, time/sleep, fs reads of pipes, ...) enforce deadline_ms. The break is documented in CHANGELOG with a migration snippet.

## Non-goals

- CRUSH-40's residual per-opcode timeouts (IO_READ, IO_WRITE, NET_CONNECT, PROCESS_WAIT, HOST_REQUEST)
- Fuel budgets (CRUSH-41/42, ruling 47)
- Preempting a cap from outside
- Out-of-tree migration tooling

## Files

- crates/crush-vm/src/host.rs
- crates/crush-lang-sdk/src/
- crates/crush-net/src/
- crates/crush-web/src/
- crates/crush-gui/src/caps.rs
- crates/crush-caison/src/vm_cap.rs
- crates/crush-lang-*/src/sdk.rs
- crates/crush-aot/src/bin/walk_run.rs
- crates/crush-vm/tests/host_deadline.rs (new)
- CHANGELOG.md

## Done when

- the trait has no default body for call_with_deadline and cargo check --workspace --all-targets passes (one CARGO_TARGET_DIR, debug only)
- cargo test -p crush-vm --test host_deadline passes: a test HostCap that blocks returns HostCapError::Timeout within deadline_ms + 250 ms through CAP_CALL on the scheduler and on PortableVm (one assertion per engine)
- cargo test -p crush-lang-sdk deadline passes: every in-tree cap whose effects() includes net/*, proc/* or time/sleep is exercised with a 50 ms deadline against a stalled backend and returns within the slack
- CHANGELOG has the entry with the migration snippet, and CRUSH-40 notes the trait half is done
- cargo +1.100.0 clippy --workspace --all-targets -- -D warnings is clean

## Depends on

- CRUSH-461

## Refs

- CRUSH-40
- CRUSH-19
- ruling 23
- ruling 34
- #401

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
