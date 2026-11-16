# CRUSH-472 — JIT refuses missing/ungranted capability calls like FastVM (#436)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-472 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#472](https://github.com/nixpt/crush-ast/issues/472) |
| **Size** | M |
| **Dependencies** | CRUSH-454, CRUSH-471 |

## Problem

#436: with str.ord not granted, FastVM returns InvalidCapability but the JIT finishes Ok. In the JIT's OP_CAP_CALL helper (crates/crush-jit/src/runtime.rs:927-984), a null ctx.capabilities (:951-954) or a cap_idx absent from the capability vector (:981-983) pushes JitValue::null() and continues without setting ctx.error, so the program keeps running on a fabricated null. FastVM returns FastError::InvalidCapability(idx) for the same case (fastvm/execution.rs:228-231). A failing cap sets ctx.error=4, rendered as the opaque 'Capability call failed'.

## Evidence (at origin/main 7ecd2515)

- issue #436
- crates/crush-jit/src/runtime.rs:927-984
- crates/crush-vm/src/fastvm/execution.rs:224-231
- crates/crush-jit/src/compiler.rs:1649-1654

## Goal

Every JIT capability call that FastVM would refuse (missing or ungranted index, null capability table) sets a distinct JIT error surfacing as the same refusal class FastVM returns. No capability call yields a fabricated null.

## Non-goals

- JIT default/off switch
- AOT capability checks (CRUSH-AOT-CAP-PARITY-AUDIT)
- JIT step/wall quotas

## Files

- crates/crush-jit/src/runtime.rs
- crates/crush-jit/src/lib.rs
- crates/crush-jit/tests/ungranted_caps.rs (new)

## Done when

- cargo test -p crush-jit --test ungranted_caps passes: str.ord and each other cap reachable through OP_CAP_CALL, run with an empty capability vector on FastVM and on the JIT, both return the InvalidCapability refusal class (the JIT message names the capability index)
- each test uses assert_jit_compiled (compilation_count() >= 1), so a silent FastVM fallback cannot pass
- a granted str.ord run gives the same result on the JIT and FastVM
- cargo +1.100.0 clippy -p crush-jit --all-targets -- -D warnings is clean

## Depends on

- CRUSH-454
- CRUSH-471

## Refs

- #436
- ruling 26

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
