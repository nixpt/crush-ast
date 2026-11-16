# CRUSH-465 — FastVM index reads raise catchable bounds/index/type errors instead of null

| Field | Value |
|-------|-------|
| **ID** | CRUSH-465 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#465](https://github.com/nixpt/crush-ast/issues/465) |
| **Size** | M |
| **Dependencies** | #448, CRUSH-453 |

## Problem

FastOp::Index pushes Null for an out-of-range array index, a non-Int array index, an out-of-range or non-Int string index, and any non-container receiver (fastvm/execution.rs:585-630). The scheduler raises ArrayBounds / BadIndex / TypeError for the same reads (scheduler.rs:1329-1366, 2325-2343) and PortableVm does the same. A bounds bug reads as null on FastVM: a silently wrong result. Those VM errors are catchable (vm.rs:203-204), so FastVM's replacement must be too. Negative-index handling is PR #448.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/fastvm/execution.rs:53-58, 585-630
- crates/crush-vm/src/scheduler.rs:1329-1366, 2325-2343
- crates/crush-vm/src/vm.rs:203-204
- crates/crush-vm/src/fastvm/instructions.rs:1076,1322 (arr_get lowers to Index)
- issue #445, PR #448

## Goal

FastVM Index on arrays and strings raises errors the tier1 harness classes as ArrayBounds (out of range), BadIndex (non-Int) and TypeError (non-container), with the VMs' wording, each catchable on FastVM exactly as on the VMs. FastVM tests relying on the null result are updated.

## Non-goals

- Map index semantics
- Negative-index normalisation (PR #448)
- The scheduler/PortableVm arr_get capability (CRUSH-ARR-GET-INDEX-SEMANTICS)
- JIT and AOT reads

## Files

- crates/crush-vm/src/fastvm/execution.rs
- crates/crush-vm/src/fastvm/types.rs
- crates/crush-lang-sdk/src/differential.rs (ErrorClass mapping)
- crates/crush-lang-sdk/tests/tier1_index_reads.rs (new)

## Done when

- tier1_index_reads: a[10], a[-10], a['x'], s[10], s['x'], 5[0] give the same ErrorClass on scheduler, PortableVm and FastVM, and the #445 entry in tier1_agreement is un-ignored
- try { a[10] } catch (e) { ... } is caught on all three engines (one assertion per engine)
- cargo test -p crush-vm fastvm passes
- cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- #448
- CRUSH-453

## Refs

- #445
- ruling H2
- ruling 17

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
