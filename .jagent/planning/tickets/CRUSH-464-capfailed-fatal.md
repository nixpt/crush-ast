# CRUSH-464 — CapFailed is fatal, not catchable, on scheduler, PortableVm and FastVM (ruling 5)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-464 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#464](https://github.com/nixpt/crush-ast/issues/464) |
| **Size** | M |
| **Dependencies** | CRUSH-453 |

## Problem

Ruling 5 (#427): capability/sandbox refusals are fatal; CapFailed behaves like CapDenied. Today VmError::is_catchable (crates/crush-vm/src/vm.rs:193-208) lists CapFailed, so try/catch swallows a failing granted capability, including fs-sandbox refusals; the doc at vm.rs:163-166 still documents the opposite #369 contract. The same function decides whether a parallel_map worker's failure is the catchable Parallel or ParallelFatal (parallel.rs:421). FastVM is worse: when cap.call returns Err it allocates the error text as a string, pushes it as the call's result and continues ('TODO: Better error handling', fastvm/execution.rs:241-246), a silently wrong result where the VMs raise CapFailed.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/vm.rs:163-166, 193-208, 847
- crates/crush-vm/src/parallel.rs:421
- crates/crush-vm/src/scheduler.rs:2797; crates/crush-vm/src/portable_vm.rs:2635
- crates/crush-vm/src/fastvm/execution.rs:53-58, 241-246
- ruling 5, issue #427

## Goal

Remove CapFailed from is_catchable: on scheduler and PortableVm a try/catch around a failing granted capability aborts with CapFailed, and a CapFailed inside a parallel_map worker is ParallelFatal. FastVM turns a capability Err into a FastError variant the tier1 harness maps to ErrorClass::CapFailed; it is not in is_catchable_fault and nothing is pushed. The vm.rs doc states the ruling-5 contract. CHANGELOG gets a behaviour-change entry (a CLI that caught fs.read of a missing file now aborts). Tests that expected to catch CapFailed are updated and listed in the PR.

## Non-goals

- FastVM host-seam changes (CRUSH-FASTVM-HOST-SEAM-ERRORS)
- CapArity: stays catchable
- The other #427 items (unwinder test coverage, PR-body correction)
- A new catchable 'soft failure' channel
- JIT or AOT capability errors

## Files

- crates/crush-vm/src/vm.rs
- crates/crush-vm/src/parallel.rs
- crates/crush-vm/src/tests/capabilities.rs
- crates/crush-vm/src/fastvm/execution.rs
- crates/crush-vm/src/fastvm/types.rs
- crates/crush-lang-sdk/src/differential.rs (ErrorClass mapping)
- crates/crush-lang-sdk/tests/tier1_cap_errors.rs (new)
- CHANGELOG.md

## Done when

- tier1_cap_errors: a granted capability that returns Err gives ErrorClass::CapFailed on scheduler, PortableVm and FastVM (one assertion per engine)
- the same program wrapped in try/catch still aborts with CapFailed on all three engines
- a parallel_map worker whose capability fails yields ParallelFatal (unit test in parallel.rs)
- no 'TODO: Better error handling' remains at the FastVM site; the vm.rs is_catchable doc no longer lists CapFailed; CHANGELOG entry cites ruling 5
- cargo test -p crush-vm -p crush-lang-sdk passes; cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-453

## Refs

- #427
- ruling 5
- #369 / CRUSH-369 (contract being reversed)

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
