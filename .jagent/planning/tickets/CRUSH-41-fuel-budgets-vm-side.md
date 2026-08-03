# CRUSH-41 — Fuel budgets: VM-side enforcement (JIT already has fuel)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-41 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M7 |

## Problem

No instruction-count bound: a runaway program spins forever (wall-clock
timeouts only cover host caps). **Current state** (s412 triage): crush-jit
already has fuel (M2 Phase 5 Tier 1, 6d9d919); `crush-vm/src` has ZERO fuel
hits — scheduler/portable_vm/fastvm enforcement absent. Related surface
already exists: CRUSHVM-QUOTA-1 (53d296b) added `Quotas` max_stack /
max_call_depth — extend THAT, don't invent a parallel budget system.

## Approach

`Quotas::max_fuel` (default ~1B instructions); per-instruction (or per-basic-
block, if bench shows >2-3% overhead) decrement in scheduler, portable_vm,
fastvm; exhaustion → `VmError::FuelExhausted`. Verify JIT tick-equivalence
against the existing crush-jit fuel so a program metering N fuel interpreted
meters ~N jitted. Tests: infinite loop halts with FuelExhausted on every
engine; differential fixture once CRUSH-77 lands.

## Definition of done

- [ ] All three VM tiers enforce max_fuel via Quotas; JIT equivalence asserted
- [ ] Overhead measured + quoted (bench before/after)
- [ ] `cargo test -p crush-vm -p crush-jit` green

## Files in scope

- `crates/crush-vm/src/{scheduler,portable_vm,fastvm/*}.rs`, Quotas type; crush-jit fuel reconciliation

## Gates

None.
