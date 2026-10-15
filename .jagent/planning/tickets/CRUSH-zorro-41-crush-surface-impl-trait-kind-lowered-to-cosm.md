# CRUSH-zorro-41 — COSM K1.2 — Crush surface: `impl`, `trait`, `@kind(…)` lowered to COSM

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-41 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | COSM K1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-zorro-40 (for the lowering step; parser/CAST work can start immediately) |
| **Estimated effort** | L |
| **Filed by** | vega — 2026-10-15, COSM phase K1 (owner decisions recorded in the design doc and dejavue) |

## Problem

Crush is the first COSM producer (owner decision 4). Today Crush has `struct` (a parse-time field check; at run
time a plain map whose name is dropped) and no methods, traits or way to choose object semantics.

**Crush's own KindDecl defaults** (from the design doc's *Crush's own KindDecl*):
- storage `sealed` (declared fields);
- semantics **`reference`** (today's behaviour; no existing program changes meaning);
- resolution `none` (traits provide default methods);
- receiver explicit `self` (`explicit_ref`, since reference semantics);
- construct `literal` (`P { … }`; `fn new` is a convention, a static member);
- equality `structural` (as maps compare today).

**`@kind(…)` on a struct selects any other strategy** (e.g. `@kind(semantics: value(copy))`,
`@kind(storage: open_dict, resolution: c3, parents: [Base])`). That makes Crush the one language that can write
*any* KindDecl: the textual way to test every strategy before the Python/JS walkers lower to COSM.

## Success criteria

- [ ] **Parser + CAST** (crush-frontend, crush-cast):
  - `impl P { fn m(self, …) { … } fn new(…) -> P { … } }`;
  - `trait T { fn m(self) -> R; fn d(self) { /* default */ } }`;
  - `impl T for P { … }`;
  - `@kind(key: value, …)` on `struct`.
  New CAST nodes (`ImplBlock`, `TraitDef`, and a kind annotation on `StructDef`) with renderer support (round-trip `render` tests like the existing ones).
- [ ] **Lowering to COSM:** all `impl` blocks for a type (anywhere in the file) merge into one `KindDecl`; trait impls add `conforms` + `Trait::method` members; methods become ordinary lifted functions with `self` as the first param; the program carries a `CosmModule`. Validation errors from `crush_cosm::validate` become compile errors with spans.
- [ ] **Semantic checks** at compile time:
  - unknown `self` usage outside an impl;
  - a missing trait method without a default;
  - duplicate impl members;
  - an `@kind` key or value not in the catalogue (a loud error listing valid values).
- [ ] Struct literal and `new P` keep working unchanged for `struct`s without impls.
- [ ] Tests: parse/render round-trips; COSM output snapshots for `Point` with `impl` + `trait` + an `@kind(semantics: value(copy))` struct; every compile-time error above has a test.
- [ ] This ticket stops at **producing** COSM. Running methods is K1.4/K1.5. Programs using `impl`/`trait` may fail at run time with a clear "methods need COSM K1.4" error until those land, never silently.

## Technical approach

- Reuse the struct pre-scan (`scan_struct_decls`) pattern to collect impls across the file.
- Keep `self` an ordinary parameter name in CAST, so closures and captures (CRUSH-251) need no special casing.

## Files to modify

- `crates/crush-frontend/src/parser/*`, `compiler.rs` (lowering entry)
- `crates/crush-cast/src/lib.rs`
- `crates/crush-frontend/src/render.rs`

## Non-goals

- Generics; trait objects (`dyn`); operator traits (K2 `OP`).
- Any VM change.

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
