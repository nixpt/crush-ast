# CRUSH-490 — io.eprint output survives parallel_map workers and __repr__/__str__ hooks

| Field | Value |
|-------|-------|
| **ID** | CRUSH-490 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#490](https://github.com/nixpt/crush-ast/issues/490) |
| **Size** | S |
| **Dependencies** | CRUSH-464 |

## Problem

Nested scheduler runs merge only VmResult::output; stderr is discarded, so io.eprint inside a parallel_map worker or a __repr__/__str__ hook vanishes with no error (#388). The scheduler appends to err_parts at the eprint cap (scheduler.rs:2514-2523) but parallel.rs:407-431 merges only output, and the nested run_from for hooks (scheduler.rs:895) does the same. Silently lost output is a wrong result on the reference engine.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/scheduler.rs:602, 895, 2514-2523
- crates/crush-vm/src/parallel.rs:407-431
- issue #388

## Goal

Nested scheduler runs merge their stderr into the parent's in print order, with the output quota counted once.

## Non-goals

- PortableVm/FastVM parallel_map (spawn/parallel is experimental under ruling 27)
- Changing stdout ordering

## Files

- crates/crush-vm/src/parallel.rs
- crates/crush-vm/src/scheduler.rs

## Done when

- scheduler test: io.eprint inside a parallel_map worker and inside __repr__ appears in VmResult::stderr, in print order
- a PortableVm test pins what PortableVm does for io.eprint inside a __repr__ hook (same stderr, or an #[ignore] with an issue number if it differs)
- existing stdout ordering tests still pass
- cargo +1.100.0 clippy -p crush-vm --all-targets -- -D warnings is clean

## Depends on

- CRUSH-464

## Refs

- #388
- CRUSH-241

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
