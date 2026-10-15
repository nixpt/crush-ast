# CRUSH-zorro-43 — COSM K1.4 — VM runtime: Records, Kinds, member ops, CONSTRUCT, receivers

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-43 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | COSM K1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-42 |
| **Estimated effort** | L |
| **Filed by** | vega — 2026-10-15, COSM phase K1 (owner decisions recorded in the design doc and dejavue) |

## Problem

The scheduler VM and the portable VM gain the runtime half of COSM: Kinds instantiated from the program's kind
table, `Record` values that point at them, and the generic member ops. This is also the real fix for
CRUSH-zorro-37: a struct pattern tests the value's Kind.

## Success criteria

- [ ] `Value::Record(Rc<RecordValue>)` with `kind` + slots laid out by the Kind's sealed fields (an overflow map for `open_dict`), and `Kind` values built from `CosmModule` at load (C3 MRO precomputed via `crush_cosm::c3_mro`).
- [ ] New CASM ops + CVM1 opcodes, on **both** the scheduler and the portable VM (crush-web):
  - `GET_MEMBER`, `SET_MEMBER`, `HAS_MEMBER`, `DEL_MEMBER`;
  - `CALL_MEMBER name argc`;
  - `CONSTRUCT kind argc`;
  - `GET_SLOT i` / `SET_SLOT i` (static fast path).
- [ ] Lookup follows the Kind's `resolution` (`none` and `c3` in K1; `prototype_chain`/`embedding_promotion` may be loud until their walkers arrive).
- [ ] Receiver strategies: `explicit_value` (copy for value Kinds) / `explicit_ref` / `explicit_ref_mut` / `bound_self` (GET_MEMBER returns a bound method: a `Func` with a receiver slot) / `bound_cls` / `dynamic_this` (basic).
- [ ] Value semantics: `value(copy)` Records copy on assignment, argument passing and return (the compiler emits the copies; the VM provides `COPY`), and `clone` deep-copies. `value(move)` use-after-move checking is compile-time and can come later (it's Rust-only); for now treat `move` like `copy` at run time and document it.
- [ ] `TYPEOF` of a Record returns its Kind name; printing uses the `str`/`repr` hooks if present, else Crush's struct print.
- [ ] **Struct patterns test the Kind** (`match v { P { … } => …, Q { … } => … }`), which **closes CRUSH-zorro-37**: add the zorro-37 program as a test.
- [ ] Tests on both VMs (`run_both`-style): construction, field get/set, method calls with each receiver strategy, a C3 diamond lookup through `@kind(resolution: c3, parents: …)` from zorro-41, `value(copy)` vs `reference` aliasing, bound methods extracted as values, pattern matching on Kind.

## Technical approach

- Keep `Value::Map` for plain dictionaries and object literals; only declared types become Records.
- Inline caches are K4; correctness first.

## Files to modify

- `crates/crush-vm/src/{vm.rs,scheduler.rs,portable_vm.rs,bytecode.rs}`
- `crates/casm/src/lib.rs`
- `crates/crush-lang-sdk/src/compile.rs`
- `crates/crush-frontend/src/compiler.rs` (pattern match on Kind)

## Non-goals

- FastVM/JIT/AOT execution (they stay loud via zorro-42).
- Operator hooks / `OP` (K2).

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
