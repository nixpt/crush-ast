# CRUSH-466 — arr_get capability on scheduler and PortableVm gets the index-op contract (negative counts from end, out of range and non-Int raise)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-466 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#466](https://github.com/nixpt/crush-ast/issues/466) |
| **Size** | S |
| **Dependencies** | #448, CRUSH-453, CRUSH-465 |

## Problem

An explicit arr_get call (a walker intrinsic) runs the scheduler/PortableVm "arr_get" capability, which casts with `Value::Int(i) => *i as usize` (a negative index wraps to a huge value, read is Null), reads slot 0 for any non-Int index (`_ => 0`), and returns Null when out of range. The index op (ARR_GET) raises ArrayBounds/BadIndex and counts negatives from the end. #446 records all three cases. FastVM routes arr_get through FastOp::Index.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/scheduler.rs:2740-2760, 2325-2343
- crates/crush-vm/src/portable_vm.rs:2569-2582
- crates/crush-vm/src/fastvm/instructions.rs:1076,1322
- crates/crush-aot/src/codegen_c.rs:2281
- issue #446

## Goal

The scheduler and PortableVm arr_get capability reuse the same need_array_index + wrap_index helpers as ARR_GET and the post-#448 arr_set (one shared function, not two copies): negative counts from the end, out of range raises ArrayBounds, non-Int raises BadIndex.

## Non-goals

- arr_set (PR #448)
- FastVM (CRUSH-FASTVM-INDEX-BOUNDS)
- ARR_GET opcode string indexing
- Map keys
- AOT arr_get out-of-range reads (Tier-2)

## Files

- crates/crush-vm/src/scheduler.rs
- crates/crush-vm/src/portable_vm.rs
- crates/crush-lang-sdk/tests/tier1_arr_get.rs (new)

## Done when

- tier1_arr_get: arr_get(a,-1), arr_get(a,-len), arr_get(a,-len-1), arr_get(a,9), arr_get(a,'x'), arr_get(a,1.5), arr_get(a,true) agree on scheduler, PortableVm and FastVM (values and ErrorClass)
- grep shows no `_ => 0` index arm left in either arr_get capability
- cargo +1.100.0 clippy -p crush-vm --all-targets -- -D warnings is clean

## Depends on

- #448
- CRUSH-453
- CRUSH-465

## Refs

- #446
- ruling H2

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
