# CRUSH-zorro-42 — COSM K1.3 — program plumbing: CASM and CVM1 carry the kind table

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-42 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | COSM K1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-40 |
| **Estimated effort** | M |
| **Filed by** | vega — 2026-10-15, COSM phase K1 (owner decisions recorded in the design doc and dejavue) |

## Problem

A compiled program has to carry its COSM declarations from the compiler to every engine, as it carries functions
and the manifest today. Engines that don't implement COSM yet must refuse such programs loudly instead of running
them with objects treated as maps.

## Success criteria

- [ ] `casm::Program` gains `cosm: Option<CosmModule>` (serde, backward compatible: absent = none).
- [ ] CVM1 bytecode (`crates/crush-vm/src/bytecode.rs`) gains a kind-table section (versioned; old blobs still load). `casm_to_vm` writes it; the disassembler prints it; `Program::from_blob`/`to_blob` round-trip it.
- [ ] **Loud elsewhere:** FastVM, the JIT, crush-aot (Rust + C) and crush-aotc refuse a program with a non-empty kind table ("COSM objects aren't supported on <engine> yet (CRUSH-zorro-43)"), with a test for each.
- [ ] Tests: round-trip a program with the Point/Shape fixture from zorro-40 through CASM JSON → CVM1 blob → load; old blobs without the section still load.

## Technical approach

- Follow how the manifest/permissions section is versioned in the CVM1 container.

## Files to modify

- `crates/casm/src/lib.rs`
- `crates/crush-vm/src/bytecode.rs`, assembler/disassembler
- `crates/crush-lang-sdk/src/compile.rs` (`casm_to_vm`)
- FastVM/JIT/AOT entry points (refusal only)

## Non-goals

- Executing anything (K1.4).

## Dispatch

**Read first:** `docs/design/object-model.md` (PR #206: **COSM — the Crush Object Semantic Model**), especially *COSM: objects are lowered, like code*, *The strategy catalogue*, *Worked
examples* and *Decisions*. If #206 isn't merged yet:
`git show origin/agent/vega/crush-kinds-design:docs/design/object-model.md`.

**The bar** (as for the parity work): the scheduler VM and the portable VM are exact. FastVM, JIT and AOT either
implement the feature or refuse **loudly at load/compile time**. Nothing is ever silently wrong, and anything
unsupported names what isn't supported.

**Rules:**
- Work in a worktree under `crush-ast/.jagent/worktrees/<you>` (a sibling of the `buckets` symlink).
- `CARGO_TARGET_DIR=/workspace/scratch/builds/<you>`, `CARGO_BUILD_JOBS=6` (14 GB RAM shared).
- Commit trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- A ready-for-review PR; never force-push or merge.
- If GitHub shows a conflict only in `.dejavue/*` (merge=union), merge `origin/main` locally.
- **Update the parity harness lists in the same PR** if your change flips any corpus entry
  (`crates/crush-lang-js/tests/node_corpus_test.rs`, `crates/crush-aot/tests/js_parity.rs`, `python_parity.rs`).
- Gates: the crates you touch, plus `-p crush-vm -p crush-lang-sdk -p crush-frontend -p crush-aot`, the walker set
  (the `Test (walkers)` CI job's package list), clippy on touched crates, and `git diff --check`.
