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


## Dispatch

Imported from `workspace-meta/prompts/crush-backlog/CRUSH-41.txt` on 2026-08-24 so this tracked ticket contains the dispatch prompt metadata.

- Branch: `agent/horse/CRUSH-41`
- Repo: `crush-ast`
- Turns: `80`
- Runner: `claude`
- Scope: this ticket file is the canonical implementation spec; read it before changing code.
- Discipline: verify the repro against current `main` first, commit and push each meaningful unit, and update this ticket with the result.
- Verification: satisfy this ticket's Definition of done, include test evidence, and quote the real post-commit `HEAD` hash.
- Lane guard: avoid `crates/crush-vm/src/fastvm/` and `crates/crush-vm/src/python.rs` unless this ticket explicitly scopes them; flag `crush_cast::Function`/`Program` shape changes before landing.
- Halt: stop and DM foreman if gates are unmet, scope is wrong, the repro no longer exists, sandbox blocks required work, or budget is nearly exhausted.
