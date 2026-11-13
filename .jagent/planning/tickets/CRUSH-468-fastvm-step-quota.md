# CRUSH-468 — FastVM honours Quotas::max_steps (StepQuota, not a fixed 1M BudgetExhausted)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-468 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#468](https://github.com/nixpt/crush-ast/issues/468) |
| **Size** | M |
| **Dependencies** | CRUSH-453 |

## Problem

No FastVM entry point takes the embedder's Quotas. FastVM::run(budget: u32) (fastvm/mod.rs:136) is a cooperative-yield slice ending in FastYield::BudgetExhausted (mod.rs:208). The public runner run_fastvm_with_caps (vm.rs:1160-1174), used by crush-vm-capi (lib.rs:127), crush_macros via run_casm_json, and the differential harness, hard-codes vm.run(1_000_000). So Quotas::max_steps is ignored: a smaller limit is not honoured, a larger one is cut at 1M, and the outcome is BudgetExhausted, not the StepQuota error scheduler and PortableVm return (vm.rs:25). Issue #401; ruling 23 puts Tier-1 run bounds in the 1.0 promise.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/fastvm/mod.rs:136, :208
- crates/crush-vm/src/vm.rs:25, 1008, 1160-1174
- crates/crush-vm-capi/src/lib.rs:127
- crates/crush-jit/src/lib.rs:144-147
- issue #401; ruling 23

## Goal

A Quotas-aware FastVM runner (e.g. run_fastvm_with_quotas(program, caps, &Quotas)) loops FastVM::run slices until Finished/Error or until quotas.max_steps instructions have run, then returns a step-quota error: a named FastError variant or documented mapping that the tier1 harness classes as ErrorClass::StepQuota. run_fastvm_with_caps and the C API route through it with Quotas::default().

## Non-goals

- Changing FastVM::run(budget) semantics
- Wall-clock enforcement (CRUSH-FASTVM-WALL-CLOCK)
- Fuel budgets / deterministic mode (ruling 47)
- AOT quotas (documented Tier-2 gap, checkpoint 1)
- JIT step counting

## Files

- crates/crush-vm/src/vm.rs
- crates/crush-vm/src/fastvm/mod.rs
- crates/crush-vm/src/fastvm/types.rs
- crates/crush-vm-capi/src/lib.rs
- crates/crush-lang-sdk/src/differential.rs
- crates/crush-vm/tests/fastvm_step_quota.rs (new)

## Done when

- cargo test -p crush-vm --test fastvm_step_quota passes: `while true {}` with max_steps=10_000 gives the step-quota error on FastVM and VmError::StepQuota on the scheduler and on PortableVm, and FastVM stopped within one slice of 10_000 instructions
- the same test with max_steps=5_000_000 on a loop of about 2M iterations finishes on FastVM (the old 1M cap is gone)
- the tier1 harness classes FastVM's outcome as ErrorClass::StepQuota, agreeing with the VMs; cargo test -p crush-lang-sdk differential passes
- cargo +1.100.0 clippy -p crush-vm -p crush-vm-capi -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-453

## Refs

- #401
- ruling 23
- CRUSH-41

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
