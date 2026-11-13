# CRUSH-474 — AOT (C and Rust): an out-of-range or non-Int array write raises instead of being skipped

| Field | Value |
|-------|-------|
| **ID** | CRUSH-474 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#474](https://github.com/nixpt/crush-ast/issues/474) |
| **Size** | S |
| **Dependencies** | #448, CRUSH-473 |

## Problem

After #448, a[9] = v on a 3-element array is an error on the scheduler, PortableVm, FastVM and the JIT. Both AOT backends skip the write silently and carry on: in C, both the ARR_SET opcode and the arr_set cap_call do `if (__wi >= 0) { ... }` with no else (a non-Int index is skipped too); in Rust both "arr_set" arms do `if let Some(j) = wrap_index(...)` with no else. #447 prefers a loud error, since AOT already has an error path (_crush_arith_error). AOT is Tier-2, but a silently wrong result outranks a loud refusal.

## Evidence (at origin/main 7ecd2515)

- crates/crush-aot/src/codegen_c.rs:2206, 2284
- crates/crush-aot/src/codegen.rs (both "arr_set" arms, per #447)
- issue #447

## Goal

Both AOT backends raise a bounds error for an out-of-range arr_set and an index error for a non-Int index, on both the opcode and the capability path, so the outcome class matches the VMs in differential_aot.

## Non-goals

- Tier-1 engines
- AOT out-of-range reads that push null (codegen_c.rs:2203,2281): file a follow-up issue if not fixed here
- AOT step and wall-clock quotas

## Files

- crates/crush-aot/src/codegen_c.rs
- crates/crush-aot/src/codegen.rs
- crates/crush-aot/tests/differential_aot.rs

## Done when

- differential_aot: `let a=[1,2,3]; a[9]=1` and `a['x']=1` are rejected on AOT Rust and AOT C and match the VMs' outcome class via assert_all_backends_agree
- cargo test -p crush-aot differential_aot passes
- cargo +1.100.0 clippy -p crush-aot --all-targets -- -D warnings is clean

## Depends on

- #448
- CRUSH-473

## Refs

- #447
- ruling H2
- ruling 17 (AOT Tier-2)

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
