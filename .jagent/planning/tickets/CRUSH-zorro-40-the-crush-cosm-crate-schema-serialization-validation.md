# CRUSH-zorro-40 — COSM K1.1 — the `crush-cosm` crate: schema, serialization, validation

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-40 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | COSM K1 |
| **Assignee** | unassigned |
| **Dependencies** | none — start here |
| **Estimated effort** | M |
| **Filed by** | vega — 2026-10-15, COSM phase K1 (owner decisions recorded in the design doc and dejavue) |

## Problem

COSM (the Crush Object Semantic Model) is the lowered, declarative object model: the object-side sibling of
CAST. Every later COSM ticket needs its types. This ticket creates them as a standalone crate, with no VM or
compiler changes.

## Success criteria

- [ ] New crate `crates/crush-cosm` (workspace member; deps: serde, serde_json, `crush-cast` only if needed for `CastType` field types; **no** dependency on crush-vm/frontend).
- [ ] Types exactly as in the design doc's schema:
  - `KindDecl { name, home_lang, span, storage, semantics, parents, resolution, receiver, construct, members, hooks, conforms, equality }`;
  - `InterfaceDecl { name, home_lang, members, satisfaction }`;
  - a `CosmModule { kinds, interfaces }`;
  - each strategy as a closed `enum` (Resolution, Receiver incl. `bound_cls`/`explicit_ref_mut`, Construct, Storage, Semantics, Satisfaction, Equality) and per-member receiver overrides.
- [ ] Serde round-trip (JSON, like CAST). Document the JSON shape in the crate README.
- [ ] **Validation** (`fn validate(&CosmModule) -> Vec<Diagnostic>` with spans):
  - unknown parent or interface references;
  - duplicate members;
  - a sealed field shadowed by a method;
  - **C3 linearization** (`fn c3_mro(kind) -> Result<Vec<KindRef>, MroError>`) with Python's exact error cases;
  - nominal conformance: every interface member is provided;
  - `value(move|copy)` combined with `open_dict` is rejected.
- [ ] Unit tests: the design doc's Rust `Point`/`Shape` and Python `Animal`/`Dog` worked examples as fixtures (round-trip + validate). C3 cases from Python's docs (diamond, the classic inconsistent-MRO example), with the expected orders copied from real `python3`.
- [ ] `cargo test -p crush-cosm`, clippy clean.

## Technical approach

- Pure data + algorithms. Keep the strategy enums `#[non_exhaustive]`-free and closed (the catalogue is deliberately closed).
- Reference Kinds by name within a module (`KindRef(String)`); resolution to runtime pointers happens at VM load (K1.4).

## Files to modify

- `crates/crush-cosm/**` (new)
- root `Cargo.toml` (workspace member)

## Non-goals

- Any VM, compiler or walker change.
- CAISON serialization (a later nicety; JSON first).

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
