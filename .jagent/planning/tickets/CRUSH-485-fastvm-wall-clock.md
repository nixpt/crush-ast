# CRUSH-485 — FastVM enforces Quotas::max_wall_time_ms via the shared wall deadline

| Field | Value |
|-------|-------|
| **ID** | CRUSH-485 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#485](https://github.com/nixpt/crush-ast/issues/485) |
| **Size** | M |
| **Dependencies** | CRUSH-468 |

## Problem

The scheduler and PortableVm share one run deadline (crates/crush-vm/src/wall.rs, CRUSH-186): wall::enter installs it and the step loops call wall::check every wall::CHECK_EVERY (1024) instructions. FastVM's run loop (fastvm/mod.rs:136-208) never calls into wall; the only wall-time use is handing quotas.max_wall_time_ms to the ai_native/dom_native seam (mod.rs:334-402). A FastVM run has no wall-clock bound beyond its step budget. #401; ruling 23 puts wall-clock bounds on Tier-1 engines in the 1.0 promise.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/wall.rs:16, :33, :97
- crates/crush-vm/src/fastvm/mod.rs:136-208, 334-402
- issue #401; ruling 23

## Goal

The Quotas-aware FastVM runner installs wall::enter(deadline_in(max_wall_time_ms)) and checks the deadline every wall::CHECK_EVERY instructions. It returns a wall-time error the tier1 harness classes as ErrorClass::WallTimeQuota, the same outcome as VmError::WallTimeQuota.

## Non-goals

- Step quota (CRUSH-FASTVM-STEP-QUOTA)
- wasm32 clock (not ticketed; see dropped)
- Preempting a blocking fastvm::Capability::call from outside
- A deadline parameter on fastvm::Capability
- A new check-interval constant

## Files

- crates/crush-vm/src/vm.rs
- crates/crush-vm/src/fastvm/mod.rs
- crates/crush-vm/src/fastvm/types.rs
- crates/crush-vm/tests/fastvm_wall_clock.rs (new)

## Done when

- cargo test -p crush-vm --test fastvm_wall_clock passes: a busy loop with max_wall_time_ms=50 and a very large max_steps gives the wall-time error on FastVM and VmError::WallTimeQuota on the scheduler and on PortableVm, each returned in under 2 s
- a nested-run test shows an enclosing (earlier) deadline wins, matching wall::enter semantics
- cargo +1.100.0 clippy -p crush-vm --all-targets -- -D warnings is clean

## Depends on

- CRUSH-468

## Refs

- #401
- CRUSH-186
- ruling 23

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
