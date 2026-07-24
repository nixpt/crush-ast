# CRUSH-36 — LanguageAdapter trait unification + 6-crate migration (M6)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-36 |
| **Title** | LanguageAdapter trait unification + 6-crate migration (M6) |
| **Hash** | `a41b81f` (feat: CRUSH-36 Commit 1 -- regression-resistance for the 4 ROADMAP-named stuck-FE walker crates) |
| **Status** | In Progress (Commit 1 landed `a41b81f`; Commit 2 + Commit 3 pending) |
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
