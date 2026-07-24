# CRUSH-36 — LanguageAdapter trait unification + 6-crate migration (M6)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-36 |
| **Title** | LanguageAdapter trait unification + 6-crate migration (M6) |
| **Hash** | `a41b81f` (feat: CRUSH-36 Commit 1 -- regression-resistance for the 4 ROADMAP-named stuck-FE walker crates) |
| **Status** | In Progress (Commit 1 landed `a41b81f`; Sub-Commit 1 landed `9d3c6f0`; Sub-Commit 2 landed `9593da6`; Sub-Commit 3 + Commit 3 pending) |
| **Phase** | M6 — Walker parity & multi-language completeness |
| **Assignee** | Buffy |
| **Dependencies** | M5 partial (M5 per ROADMAP dependency: partial for `@exhaustive-match-sites` lint; this ticket is independent since it touches trait surface, not walker lowering). |
| **Blocks** | CRUSH-37 (Java walker), CRUSH-39 (walker→AOT for all 12), and Publisher-lane `walker-core` publish (per ROADMAP M9 / Publish note: traits entangled with walker-core publish). |
| **Estimated effort** | 3 commits (Commit 1 narrow regression-resistance; Commit 2 the 4th-trait unification; Commit 3 the CLI binary-name + backwards-compat shim). |

## Why this exists

The ROADMAP M6 done conditions name this ticket as the foundation for M6 — once the 6 Frontend-only crates migrate onto `LanguageAdapter`, the CLI mapping bug (`crates/cli/src/main.rs`'s `py`/`pyw` → `python_walker`) is fixed, and the registry unblocks CRUSH-39 (walker→AOT for all 12 walkers).

## ⚠ Discovery at ticket-filing time (s397, jul-24)

When gathering context for this ticket, **the ROADMAP's "6 stuck-FE crates" claim turned out to be out of date**. Reading the 4 ROADMAP-named stuck-FE crates' `lib.rs` ends shows they all already have `LanguageAdapter` impls:

| ROADMAP-named crate | Real status (verified by reading `crates/crush-lang-<x>/src/lib.rs`) |
|-------|---|
| nepali | `impl_adapter_from_frontend!(NepcodeAdapter, "nepcode", &["np", "nepali"], crate::nepali_to_cast);` — macro-generated adapter ✓ |
| bash | `impl_adapter_from_frontend!(BashAdapter, "bash", &["sh", "bash"], crate::bash_to_cast);` — macro-generated adapter ✓ |
| custom | `pub struct CustomAdapter(pub CustomFrontend); impl crush_walker_core::LanguageAdapter for CustomAdapter { ... }` — manual adapter ✓ |
| rust | `impl_adapter_from_frontend!(RustAdapter, "rust", &["rs"], crate::rust_to_cast);` — macro-generated adapter ✓ |
| (python) | `impl_adapter_from_frontend!(PythonAdapter, "python", &["py", "pyw"], crate::python_to_cast);` — macro-generated adapter ✓ |
| (zsh) | `impl_adapter_from_frontend!(ZshAdapter, "zsh", &["zsh"], crate::zsh_to_cast);` — macro-generated adapter ✓ |

**0 of the 6 ROADMAP-named stuck-FE crates are actually stuck on Frontend today.** The migration has already happened. What remains for CRUSH-36 is the **work the migration enables but didn't trigger**:

1. **Active regression-resistance tests** — without a test that exercises the `AdapterRegistry::languages()` lookup of each migrated adapter, the macro-generated impl could silently regress (delete the macro line and the crate still compiles against `Frontend` alone). The CRUSH-32/CRUSH-33/CRUSH-34 schema established the precedent of "active surface-parity test in the new module" — apply it to each migrated walker crate.

2. **The 4th trait** (`LanguageWalker`) in `crates/crush-frontend/src/language_walkers.rs:9` is **separate** from `LanguageAdapter` (in `crates/crush-walker-core/src/lib.rs:567`). Today's CLI registers `SubprocessWalker: LanguageWalker` and dispatches by `WalkerRegistry` (separate from `AdapterRegistry`); the `LanguageAdapter` registry isn't yet wired into the CLI dispatch path. Unification is the real architectural work.

3. **The CLI binary-name bug** (the ROADMAP's most concrete bug): `crates/cli/src/main.rs:19-30` uses string binary names (`python_walker`, `js_walker`, `rust_walker`, etc.) — these either don't match the actual binaries the walker crates build to (e.g., go/zig/wasm use `crush_lang_<name>` per the same files; mixed naming across crates), OR reference binaries that aren't buildable as a separate package. The right fix is to map extensions → `AdapterRegistry::walk(ext)` instead of map extensions → subprocess binary. That requires Commit 2 (4th-trait unification) to land first.

The ROADMAP M6 done condition decomposes into these 3 sub-tasks better than the original "migration" framing suggested.

## Decomposition (3 commits, narrowed)

### Commit 1: regression-resistance (THIS COMMIT, first user-instructed scope)

- File this ticket (just done)
- For each of the 4 ROADMAP-named stuck-FE crates (nepali, bash, custom, rust), add a per-crate `mod tests` block with one active test that registers that crate's `LanguageAdapter` impl into an `AdapterRegistry` and asserts the language name appears in `registry.languages()`. Nepali + Bash get a fresh `mod tests` block (none today); Custom + Rust augment their existing `mod tests` blocks. Total 4 edits, ~30 insertions across 4 files.
- Validation: cargo check / cargo test stays GREEN across the 4 walker crates; their adapter macro/manual impls stay functional.

### Commit 2: 4th-trait unification (DEFERRED, the real architecture work)

- Decision on trait-shape (4-options analysis from `crates/crush-walker-core/src/lib.rs` + `crates/crush-frontend/src/language_walkers.rs`):
  - Option A: `Frontend: LanguageAdapter` (supertrait) — adds impl bound to existing FE impls.
  - Option B: `LanguageAdapter: Frontend` (supertrait inverse).
  - Option C: New unified `CrushAdapter` trait, both old traits become re-export aliases.
  - Option D: Macro `impl_both_for!` generates `impl Frontend` AND `impl LanguageAdapter` from one trait impl.
- Plus unification with `LanguageWalker` (`crates/crush-frontend/src/language_walkers.rs`).
- Plus `AdapterRegistry` becomes the single registry surfaced by the CLI dispatch (replacing the SubprocessWalker mapping in `crates/cli/src/main.rs`).

### Commit 3: CLI binary-name mapping fix (DEFERRED, the most concrete user-visible bug)

- Rewrite `crates/cli/src/main.rs`'s `walker_binary()` to route via `AdapterRegistry::walk(source, filename)` instead of subprocess-binary-name lookup.
- 6 broken string mappings (rust, python, js, c, bash, zsh; plus go/zig/wasm which had different-shape binary names) all collapse into one registry call.
- This commit lands AFTER Commit 2 because it depends on the unified registry.

## Files to modify (planned)

| File | Commit | Change |
|------|--------|--------|
| `crates/crush-lang-nepali/src/lib.rs` | 1 | +~9 lines: `mod tests` block with `nepcode_adapter_registers_in_unified_registry` |
| `crates/crush-lang-bash/src/lib.rs` | 1 | +~9 lines: `mod tests` block with `bash_adapter_registers_in_unified_registry` |
| `crates/crush-lang-custom/src/lib.rs` | 1 | +~9 lines (inside existing `mod tests`): `custom_adapter_registers_in_unified_registry` |
| `crates/crush-lang-rust/src/lib.rs` | 1 | +~9 lines (inside existing `mod tests`): `rust_adapter_registers_in_unified_registry` |
| `crates/crush-walker-core/src/lib.rs` (or `adapter.rs`) | 2 | `Frontend: LanguageAdapter` (or Option B/C/D — TBD in commit-2 thinker run) |
| `crates/crush-frontend/src/language_walkers.rs` | 2 | `LanguageWalker: LanguageAdapter` (or supertrait alternative) |
| `crates/cli/src/main.rs` | 2/3 | Replace `walker_binary()` subprocess lookup with `AdapterRegistry::walk()` |

## Success criteria (per commit)

| Commit | cargo check | cargo test | Tests expected | Regression |
|--------|-------------|------------|----------------|------------|
| 1 | per-crate GREEN | `crush-lang-{nepali,bash,custom,rust} --lib` | 4 new active tests all GREEN; existing tests still GREEN | other walkers (c/python/js/go/zig/wasm/dart) unchanged GREEN |
| 2 | `--workspace` GREEN; per-crate GREEN | `--lib` for `crush-walker-core` + dependents | active unification tests pass; recompile-check on all 12 walkers no breakage | `python_to_cast` etc. still callable; no Frontend-only call sites break |
| 3 | `--workspace` GREEN | `--lib cli + walk_compute` (if it exists); `--bin clis tests` | end-to-end CLI dispatch via the registry round-trips for at least 3 file extensions | existing CLI tests still GREEN |

## Out of scope (intentional non-goals)

- **WASM / Dart / Zig walker crates reaching 1.0** (per ROADMAP M6 done-condition item 5). Currently 0.1.0; not in CRUSH-36 scope. (Belongs in CRUSH-49+ or a separate version-sync ticket per ROADMAP Publish lane.)
- **Java + Kotlin walkers** (CRUSH-37 / CRUSH-38).
- **The 7 remaining walker-lowering gaps** from VISION.md (CRUSH-35; independent of CRUSH-36).
- **Real-data fix to the CLI binary-name lookup for non-`crush-lang-*` walker binaries**. CRUSH-36 Commit 3 collapses the dispatch into the registry — `python_walker` / `bash_walker` strings go away entirely.

## Reviewed forward flags (preemptive, for per-commit review rounds)

1. **Test-mod placement in `crush-lang-nepali/src/lib.rs`**: currently NO `mod tests` block. Adding one introduces a `dev-dependency` consideration — but `crush-walker-core` is a regular dependency (used for `Frontend` already); no `dev-dependencies` Cargo.toml change required since the test uses only `AdapterRegistry` (re-exported from `crush-walker-core`).

2. **`crush-lang-custom/src/lib.rs` test-mod expansion**: existing `test_custom_dsl_frontend` test uses `CustomFrontend::from_cson` + `parse_to_program` directly (not via the Frontend pipeline). The new active test can construct a `CustomAdapter(CustomFrontend { name: "custom".to_string(), extensions: vec![".custom".to_string()], rules: Vec::new() })` — minimal enough to compile.

3. **The `Frontend` → `LanguageAdapter` supertrait choice (Option A vs Option B etc.)**: deferred to Commit 2 thinker run. Commit 1 doesn't need this decision because the migration has already happened — no trait-shape change is required to GREEN-pass the 4 active tests. Capture this so Commit 2 thinker doesn't re-derive the same finding.

4. **`LanguageWalker` (in `crush-frontend/src/language_walkers.rs`) is the actually-untouched 4th trait from the ROADMAP M6 framing**. Commit 2's unification must address this, not just `Frontend`+`LanguageAdapter`.

5. **The `python_walker` / `bash_walker` / etc. CLI string-mapping bug**: definitely real (confirmed by reading `crates/cli/src/main.rs:19-30` + the 4 stuck-FE-declared crates' Cargo.toml binaries). The Cargo.toml of `crush-lang-python` builds a binary named `crush_lang_python` (per package-name-with-hyphens-to-underscores convention) — NOT `python_walker`. The fix is structural (route via registry), not string-renaming.

## Done condition

All 3 commits landed on `agent/buffy/M2-JIT-PHASES-2-4`, each with reviewer LAND-AS-IS verdict and validation GREEN. Ticket moves from `In Progress` → `Done`; `Hash` row and `Closed by` row populated with the final SHA.

## Cross-references

- CRUSH-32 (AI): CRUSH-32+CRUSH-33+CRUSH-34 schema precedent for "Commit 1 skeleton + active size+no-dup test".
- CRUSH-33 (DOM): commits `b4c81a8` (Commit 2), `08726d8` (review polish); the active-test pattern was established there.
- CRUSH-34 (Concurrency): commits `79390d7` (Commit 1), `e3f6b1f` (status); the "Process note" diagnostic lesson about test-result filter catching FAILED output.
- M6 spec: `.jagent/planning/ROADMAP.md` M6 section.
- Build-order diagram: `.jagent/planning/ROADMAP.md` Build-order section.
- Related files: `crates/crush-walker-core/src/lib.rs:113` (Frontend), `:293` (Walker), `:567` (LanguageAdapter); `crates/crush-frontend/src/language_walkers.rs:9` (LanguageWalker); `crates/cli/src/main.rs:11-30` (broken mapping).

## Notes from CRUSH-36 Commit 1 review

**Reviewer verdict** (Nit Pick Nick, s397): LAND-AS-IS after the 1-line mid-validation fix.

### 1-line fix during validation (E0425: cannot find value `RustAdapter`)

When the initial draft of Commit 1 was committed (before this review), `crush-lang-rust --tests` failed to compile with E0425 "cannot find value `RustAdapter` in this scope" at line ~206 of `crates/crush-lang-rust/src/lib.rs`. Root cause: the existing `mod tests` in `crates/crush-lang-rust/src/lib.rs` uses `use super::{rust_to_cast, RustFrontend};` at module-level (specific imports, NOT glob) — uniquely among the 4 ROADMAP-named stuck-FE crates being modified. The macro-generated `RustAdapter` struct (from `impl_adapter_from_frontend!(RustAdapter, "rust", &["rs"], crate::rust_to_cast);`) was NOT in scope inside the test fn body. The 3 sibling walker crates (`crush-lang-nepali`, `crush-lang-bash`, `crush-lang-custom`) compiled cleanly because —

- `nepali` + `bash`: their existing `mod tests` use `use super::*;` (glob import) which DOES pull in the macro-generated adapter struct.
- `custom`: uses fn-body-scoped import pattern (already had no problem because its `CustomAdapter` is a hand-written `pub struct CustomAdapter(pub CustomFrontend);` at crate root, not macro-generated; existing test mod scoped imports work).

**Fix (Commit 1 re-validation step)** (1 line, inside `rust_adapter_registers_in_unified_registry` fn body): added `use super::RustAdapter;` as a fn-scoped import, alongside the existing fn-body-scoped `use crush_walker_core::{AdapterRegistry, LanguageAdapter};`. An 8-line rustdoc `Note:` paragraph was added above the test fn to capture the WHY for future readers (CRUSH-37/38 Java/Kotlin; CRUSH-43+ future walkers).

### Non-blocking follow-up nit (#1 from reviewer)

The 8-line doc-comment paragraph in `crates/crush-lang-rust/src/lib.rs`'s `rust_adapter_registers_in_unified_registry` test references "nepali/bash which use `use super::*;`" but **does not mention `custom`** — which also uses fn-body-scoped (not glob) for the same reason as rust (custom's `CustomAdapter` is hand-written and the existing test mod uses specific imports). The reviewer noted the omission is factually incomplete.

**Polish suggestion** (does NOT block Commit 1 close): re-word the rustdoc to read "nepali/bash use `use super::*;` in their test mods; custom uses fn-body-scoped like us, so its test compiles for the same reason." Capture for the next polish followup commit.

### Validation status (post the 1-line fix)

| Crate | Compile | Test count | Result |
|-------|---------|------------|--------|
| `crush-lang-nepali --lib` | GREEN | 3/3 | PASS |
| `crush-lang-bash --lib` | GREEN | 3/3 | PASS |
| `crush-lang-custom --lib` | GREEN | 2/2 | PASS (1 pre-existing + 1 new) |
| `crush-lang-rust --lib` | GREEN | 12/12 | PASS (3 sdk::tests::* + 3 pre-existing + 1 new) |
| `crush-walker-core --lib` (regression) | GREEN | 5/5 | PASS |
| `crush-frontend --lib` (regression) | GREEN | 78/78 | PASS |

Commit 1 (`a41b81f`) is GREEN across all 6 affected crates. Reviewer verdict: LAND-AS-IS.

## Process note — fn-scoped test-mod imports (CRUSH-walker diagnostic pattern)

The E0425 root-cause discovery during `crush-lang-rust --tests` validation is a teachable lesson worth recording.

**Within the 4 ROADMAP-named stuck-FE walker crates, only 2 use `use super::*;` glob imports** in their existing `mod tests` (`nepali`, `bash`); **2 use specific imports** (`custom`, `rust`). When adding new tests that reference macro-generated structs (like `RustAdapter` from `impl_adapter_from_frontend!`), the import path differs across them.

When adding regression-resistance tests to future walker crates (CRUSH-37 Java, CRUSH-38 Kotlin, plus any CRUSH-43+ walker). Pattern to follow:

1. **First step**: check existing `mod tests` block in `crates/crush-lang-<name>/src/lib.rs`. Determine whether the existing crate uses `use super::*;` (glob) or specific imports.
2. **Glob crates** (`nepali`, `bash`-shape): the macro-generated `<Lang>Adapter` struct is auto-in-scope — no extra `use super::...;` line needed inside the test fn body.
3. **Specific-import crates** (`custom`, `rust`-shape): must `use super::<MacroGeneratedStruct>;` fn-scoped inside the test fn body. Add a short rustdoc `Note:` paragraph capturing the WHY for future readers.

This pattern is captured here so that future walker-crate test additions don't repeat the same compile-error-mid-validation cycle. The CRUSH-34 ticket process note captured a different lesson (the per-thread FAILED-output filter); this CRUSH-36 process note captures the import-path asymmetry lesson. Both are diagnostic patterns the next agent will benefit from having on hand.

## Notes from CRUSH-36 Commit 2 Sub-Commit 1 review

**Reviewer verdict** (Nit Pick Nick): LAND-AS-IS after Fix #7 full cascade closure.

### The 7-fix cascade (architectural lesson)

The Sub-Commit 1 source landed in 7 cascade fixes before reaching GREEN. The cascade is the canonical closure pattern for any future \`pub trait Subtrait: Supertrait\` supertrait-tie architecture work (Sub-Commit 2 will hit analogous cascades for \`Walker\` + \`LanguageWalker\`):

| Fix | What | Why |
|-----|------|-----|
| **#1** | \`pub trait Frontend: LanguageAdapter { ... }\` + inlined default walk body | Supertrait tie enables Frontend impls to register directly in \`AdapterRegistry\`; inlining avoids \`where Self: Sized\` which would strip \`walk\` from trait-object vtable |
| **#2** | First attempt: specific \`impl<W: Walker + Send + Sync> LanguageAdapter for TreeSitterFrontend<W>\` UFCS-dispatching to Frontend | Provisional bridge impl; LATER REDUNDANT after Fix #6 blanket |
| **#3** | Extend existing FE impl with \`+ Send + Sync\` | Send+Sync propagation from \`LanguageAdapter: Send + Sync\` to existing tree-sitter FE impl |
| **#4** | Extend \`run_walker_binary<W>\` with \`+ Send + Sync\` | Send+Sync propagation through the free fn constructing \`TreeSitterFrontend<W>\` internally |
| **#5** | \`use anyhow::Result;\` to adapter.rs test mod | CROSS-SCOPE ALIAS-DISCIPLINE hypothesis; dashed (Fix #6 was actual root cause) -- kept defensed against future test mods (CRUSH-37/38/43+) |
| **#6** | **BLANKET** \`impl<T: Frontend + Send + Sync> LanguageAdapter for T\` | **THE ARCHITECTURAL CLOSURE** -- without this, every \`impl Frontend for X\` fails the supertrait-tie impl-site bound |
| **#7** | Truncated Fix #2 + removed dashed \`use anyhow::Result;\` | E0119 coherence conflict cleanup + dashed-hypothesis cleanup |

### Process notes -- the 4 distinct diagnostic lessons

This Sub-Commit 1 captured four distinct diagnostic lessons worth codifying for future supertrait-tie work:

#### Lesson 1: Subtrait-tie Send+Sync propagation

\`pub trait Subtrait: Supertrait\` propagates ALL supertrait bounds (here \`LanguageAdapter: Send + Sync\`). The propagation runs through to:
- **every \`impl Subtrait for X\` block** (e.g., \`impl Frontend for RustFrontend\` requires \`RustFrontend: Send + Sync\`)
- **every free function that constructs a concrete subtype of the trait** (e.g., \`run_walker_binary<W: Walker>\` requires \`W: Send + Sync\` to construct \`TreeSitterFrontend<W>: Send + Sync\`)
- **Failure mode**: E0277 "X cannot be sent between threads safely" -- canonical signal that the propagation cascade needs another \`+ Send + Sync\` bound at the failing site.

#### Lesson 2: Trait default-implementing body cannot call free functions taking \`&dyn Self\`

\`fn walk(&self, ...)\` body inside \`pub trait Frontend\` calling \`frontend_pipeline(self, source)\` requires \`&Self -> &dyn Frontend\` coercion. WITHOUT \`where Self: Sized\` in scope, this coercion FAILS at the trait-body default-implementing-method site (E0277 "the size for values of type Self cannot be known at compilation time"). 

**Workaround**: add \`where Self: Sized\` -- BUT this REMOVES \`walk\` from the trait-object vtable, breaking every \`Box<dyn Frontend>::walk(...)\` and \`Box<dyn LanguageAdapter>::walk(...)\` call path (including \`AdapterRegistry::walk\`).

**Better workaround**: inline the body. The trait body just calls \`self.parse(src)?; self.analyze(&ast)?; self.lower(ast)?; Ok((report, program))\`. Functionally equivalent to \`frontend_pipeline(self, source)\` but coerces via Self-sized method calls instead.

**Trade-off**: inlining trades a thin abstraction for vtable-object-safety. The fix is intentional, not a missed abstraction.

#### Lesson 3: Cross-scope \`use anyhow::Result;\` discipline

The trait body in \`crates/crush-walker-core/src/lib.rs\` declares parse/analyze/lower signatures as bare \`Result<...>\` because lib.rs has \`use anyhow::{Context, Result};\` at file-scope -- the alias makes \`Result<...>\` resolve to \`anyhow::Result<...>\`. When ANY other crate or sub-module implements \`impl Frontend for X\`, that impl scope may NOT bring the alias in -- \`Result<...>\` in that scope resolves to \`std::result::Result<...>\`. Mismatch causes trait-method signature type-resolution failure.

**Practical rule**:
- If impl-side has \`use anyhow::Result;\` (or \`use anyhow;\`), bare \`Result<...>\` resolves to \`anyhow::Result<...>\` -- matches.
- If impl-side uses fully-qualified \`anyhow::Result<...>\`, no alias needed.
- If impl-side has NO \`use anyhow\` and uses bare \`Result<...>\`, MISMATCH.

**For Sub-Commit 2 + future walkers (CRUSH-37 Java, CRUSH-38 Kotlin, CRUSH-43+)**: add \`use anyhow::Result;\` to file scope in EVERY new walker crates
(OR use fully-qualified `anyhow::Result<...>` throughout to avoid the alias-dependency).

#### Lesson 4: Supertrait-tie without blanket impl is incomplete

The supertrait clause `Frontend: LanguageAdapter` is **only a constraint**, not an automatic synthesis. It enforces that any `impl Frontend for X` block satisfies `X: LanguageAdapter` at the impl-site — but the constraint must be SATISFIED. The two ways to satisfy it:

1. Add a per-type `impl LanguageAdapter for X` for every concrete `X` that implements Frontend (verbose, error-prone — Fix #2 was a partial attempt at this).
2. Add a blanket **`impl<T: Frontend> LanguageAdapter for T`** which automatically implements LanguageAdapter for every Frontend impl (canonical closure — Fix #6 is the actual architectural state).

**Without one of these, every `impl Frontend for X` fails E0277 "the trait bound `X: LanguageAdapter` is not satisfied"**. Future supertrait-tie commits: write the blanket as part of the FIRST edit (Fix #1 includes blanket), not as a later discovered-need (Fix #6 in this cascade).

### Forward flags (reviewer-annotated)

**F1** (most important): future Frontend impls (CRUSH-37 JavaFrontend, CRUSH-38 KotlinFrontend, CRUSH-43+) MUST rely on the Fix #6 blanket. DO NOT write per-type `impl LanguageAdapter for FrontendType` — that re-triggers E0119 against the blanket. Write `impl LanguageAdapter` only for separate wrapper types (the `<Lang>Adapter` macro pattern) where the wrapper is NOT a Frontend.

**F2**: free functions that hold a `TreeSitterFrontend<W>` (or any future Frontend type passed by reference to a `&dyn Frontend` parameter) MUST have `+ Send + Sync` on the generic if the function crosses the dyn boundary.

**F3**: idiomatic alias-discipline for new walker test mods (CRUSH-37 Java, CRUSH-38 Kotlin, CRUSH-43+): prefer fully-qualified types (`anyhow::Result<...>`) at impl sites over bare `Result<...>` resolvable only via `use anyhow::Result;` scope.

**F4**: any subsequent supertrait-tie architecture work (Sub-Commit 2's `Walker + LanguageWalker` unification) MUST include the blanket impl as part of the FIRST edit, not as a later discovered-need. Doing the blanket upfront turns the 7-fix cascade into a 1-fix forward.

**F5**: per-FE regression — the 6 existing Frontend impls (Bash/Custom/Nepali/Python/Rust/Js/Mock) all auto-derive LanguageAdapter via the Fix #6 blanket. Closure is uniform with no per-crate boilerplate.

### Validation status (post-cascade)

| Crate | Compile | Test count | Result |
|-------|---------|------------|--------|
| `crush-walker-core --all-targets` | GREEN | — | PASS |
| `crush-walker-core --lib` | GREEN | 7/7 | PASS (5 pre-existing MockAdapter + 1 new MockFrontend coercion `frontend_to_adapter_structural_coercion_via_supertrait_tie` + 1 integration) |
| `crush-walker-core --all-targets` warnings | 1 pre-existing `use std::sync::Arc;` (OUT OF SCOPE) | 0 NEW | PASS |
| `cargo check --workspace --tests` (compile-only) | GREEN | — | PASS across 11 crates |
| Per-FE compile regression (nepali/bash/custom/python/rust/js) | GREEN | — | PASS |
| Per-FE test regression (nepali/bash/custom/python/rust/js) | GREEN | — | PASS |
| AdapterRegistry consumer regression (frontend + lang-sdk) | GREEN | — | PASS |

Commit at `9d3c6f0`. Reviewer verdict: LAND-AS-IS after Fix #7 full cascade closure. The 7-fix cascade is the canonical closure pattern.

## Process notes — cascade closure pattern (M6 supertrait-tie canonical reference)

This commit cascaded through 7 fixes to reach GREEN. The cascade is now the canonical closure pattern for any future supertrait-tie architecture work. Process notes below abstract the lessons so future agents recognize the pattern early and skip the cascade.

### Cascade trajectory summary (in order of application)

| Order | Fix | Change | Status |
|-------|-----|--------|--------|
| #1 | supertrait tie | `pub trait Frontend: LanguageAdapter { ... fn walk(default inlined) ... }` | RED on E0277 (Self not Sized for default body — fixed by inlining) |
| #2 | supplementary impl | specific `impl<W: Walker + Send + Sync> LanguageAdapter for TreeSitterFrontend<W>` UFCS-dispatching to Frontend methods | REDUNDANT after #6 blanket; removed in #7 |
| #3 | propagate Send+Sync | extending existing `impl<W: Walker> Frontend for TreeSitterFrontend<W>` to `+ Send + Sync` | GREEN at this layer, RED at #4 |
| #4 | propagate Send+Sync | extending `pub fn run_walker_binary<W: Walker>` to `+ Send + Sync` | GREEN at FE impl layer, RED at #5 |
| #5 | dashed alias-scope fix | added `use anyhow::Result;` to test mod | dashed hypothesis; reverted in #7 |
| #6 | architectural closure | blanket `impl<T: Frontend + Send + Sync> LanguageAdapter for T` UFCS-dispatching to Frontend methods | CANONICAL closure; covers all concrete Frontend impls |
| #7 | cascade cleanup | truncated #2 + reverted #5 + updated doc-comments | single source of truth |

### Forward flags (same as Notes section, consolidated for cross-reference)

F1: rely on the Fix #6 blanket for new Frontend impls — DO NOT write per-type `impl LanguageAdapter for FrontendType`.

F2: free fns crossing `&dyn Frontend` boundary MUST have `+ Send + Sync` on the generic if the held type contains a non-Send+Sync field.

F3: prefer fully-qualified `anyhow::Result<...>` at impl sites over bare `Result<...>` resolvable only via `use anyhow::Result;` scope.

F4: subsequent supertrait-tie work MUST include the blanket impl as part of the FIRST edit (not as a later discovered-need).

F5: per-FE regression — 6 existing Frontend impls auto-derive LanguageAdapter via the blanket; closure is uniform with no per-crate boilerplate.

This pattern will be referenced from CRUSH-37 (Java), CRUSH-38 (Kotlin), CRUSH-43+ future walkers, plus Sub-Commit 2 (the `Walker + LanguageWalker` unification which will hit analogous cascades if the blanket impl is forgotten in Fix #1).
## Status row update for Sub-Commit 2

**Sub-Commit 2 landed at `9593da6`** (Option D: `impl_both_for_walker!` macro + cascade closure UP FRONT). The architectural decision between Cascade (Sub-Commit 1's supertrait-tie + blanket) and Closure (Sub-Commit 2's macro-generated concrete impls) is now a documented pattern for future trait-unification work.

## Notes from CRUSH-36 Commit 2 Sub-Commit 2 review

**Reviewer verdict** (Nit Pick Nick, final-final): LAND-AS-IS after addressing all 4 actionable items (G deprecation, B filename-loss doc promotion, F forward-flag renumber, D runtime parse() test) + the F5 numbering gap closure.

### The Option D architecture (closure UP FRONT)

The 4th-trait unification work uses architecture Option D -- the `impl_both_for_walker!` macro that generates BOTH `impl Walker for X` AND `impl LanguageWalker for X` from a single source-of-truth invocation. Option D was chosen over Option A (supertrait tie `Walker: LanguageWalker`) for two reasons:

1. **Cross-crate coupling**: `Walker` lives in walker-core; `LanguageWalker` lives in crush-frontend. Supertrait tie would force cross-crate dep inversion.
2. **Method-signature conflict**: `Walker::language()` returns `tree_sitter::Language` (grammar-bound, opaque); `LanguageWalker::language()` returns `&'static str` (UI-bound, polyglot-frontend-legal). Structural types don't unify cleanly.

Option D sidesteps both by generating concrete impls on a ZST. The cascade closure is **UP FRONT** -- no supertrait, no blanket, no E0119, no 6-fix cascade. This is the Sub-Commit 1 Lesson 4 application:

> Supertrait-tie without immediate blanket impl is incomplete -- write the closure structurally in Fix #1, not as a later discovered-need.

### Cascade closure trajectory (vs Sub-Commit 1's 7-fix cascade)

| Sub-Commit | Cascade | Closure pattern |
|------------|---------|-----------------|
| 1 (Frontend: LanguageAdapter) | 7 fixes | Subtrait-tie + blanket (Fix #6 in the 6th iteration) |
| 2 (Walker + LanguageWalker) | 1 fix (`#[derive(Clone, Copy)]` for E0382) | Macro with concrete impls + ZST (closure UP FRONT) |

The 1 fix in Sub-Commit 2 was the E0382 "use of moved value" error when the macro-generated ZST was moved into TWO `Box<dyn>` instances in the test. The fix: `#[derive(Clone, Copy)]` on the macro-generated struct. ZSTs are trivially copy + clone (no field to copy), so the derive is zero-cost + zero-risk.

The structural difference: Sub-Commit 1's supertrait-tie was a Cascade Architecture pattern (constraint + blanket closure); Sub-Commit 2's macro is a Closure Architecture pattern (concrete impls on ZST, no abstraction layer above the concrete impls). The Closure Architecture is structurally cascade-resilient because there's no abstraction layer to cascade through.

### Forward flags F1..F8 (consolidated)

The macro's rustdoc has F1..F8 forward flags covered. Key items:

- **F5** (no per-FE regression): the macro is OPT-IN; existing `impl Walker for X` impls in tree-sitter walkers (Go/C/Zig/Dart) continue to compile unchanged. Migration is the Sub-Commit 2 Commit B follow-up.
- **F6** (filename-loss caveat): `Walker::walk(&Tree, &[u8])` path uses empty filename (the trait signature has no filename parameter); the canonical filename-preserving flow is the `LanguageWalker::parse`/`walk` round-trip.
- **F7** (no migration in this commit): Go as the canonical exemplar is the Sub-Commit 2 Commit B follow-up.
- **F8** (test limitation): the test uses `unreachable!()` for `$ts_lang` as a test-side dummy; DO NOT call `.parse()` on the test's `MacroGenAdapter` in production code.

### Deprecation

The existing `impl_adapter_from_walker!` macro is now marked `#[deprecated(note = "use impl_both_for_walker! instead")]`. The macro predates the Sub-Commit 2 unification and is dead code (no callers in the post-Sub-Commit-1 landscape). The deprecation signalizes the migration path for any external users.

### Cross-crate dep

Added `crush-frontend` as a dev-dependency to walker-core's Cargo.toml so the test can import `crush_frontend::language_walkers`. Production builds of walker-core are NOT affected (dev-dep only). The macro itself uses `$crate::Walker` (always resolves to walker-core) and `crush_frontend::language_walkers::LanguageWalker` (resolved at call-site scope).

## Process notes -- Closure Architecture pattern (subtrait-tie without blanket)

This commit established the Closure Architecture pattern as a structural alternative to Sub-Commit 1's Cascade Architecture pattern. The 2 patterns differ in cascade-resilience:

| Pattern | Subtrait-tie | Closure | Cascade risk |
|---------|--------------|---------|--------------|
| Cascade (Sub-Commit 1) | yes (`Frontend: LanguageAdapter`) | trait blanket (`impl<T: Frontend> LanguageAdapter for T`) | 6-fix cascade if blanket is added late |
| Closure (Sub-Commit 2) | no (concrete impls) | structural (ZST + macro) | 1-fix cascade (E0382 for ZST Clone/Copy) |

The Closure Architecture is the preferred pattern for future trait-unification work in the M6 arc. Specifically:

- When 2 traits are in DIFFERENT crates (cross-crate): prefer Closure (macro) over Cascade (supertrait-tie + blanket). Avoids cross-crate dep inversion.
- When 2 traits are in the SAME crate (intra-crate): either pattern works; Cascade is more idiomatic for single-crate trait families.
- When the trait methods have STRUCTURAL conflicts (different return types for same-named methods): Closure is the only option; Cascade can't unify the methods.

The Closure Architecture's macro-generated concrete impls also enable compile-time validation via the structural-coercion test pattern (proves the SAME ZST coerces to BOTH trait-object boxes). This is the active-test pattern for trait-unification closure.

### Future work

- CRUSH-36 Sub-Commit 2 Commit B: migrate GoWalker (canonical exemplar) to use `impl_both_for_walker!`. This will replace the hand-rolled `impl Walker for GoWalker` with the macro invocation AND add the `impl LanguageWalker for GoAdapter` (the new ZST). After Commit B, Go will be in BOTH `AdapterRegistry` and `WalkerRegistry`.
- CRUSH-36 Sub-Commit 3: CLI binary-name mapping fix. The CLI's `walker_binary()` function in `crates/cli/src/main.rs:11-30` currently maps extensions to subprocess binary names (broken). Sub-Commit 3 will route via `AdapterRegistry::walk` instead, collapsing the 6 broken string mappings into 1 registry call.
