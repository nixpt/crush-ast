# CRUSH-37 — Java tree-sitter walker (M6 tier-2 expansion)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-37 |
| **Title** | Java tree-sitter walker + JavaAdapter registry entry (M6 first new tier-2 walker) |
| **Hash** | (to be populated when in-progress work commits) |
| **Status** | In Progress (Commit 1 landed; Sub-Commit 2 + Sub-Commit 3 pending) |
| **Phase** | M6 — Walker parity & multi-language completeness |
| **Assignee** | Buffy |
| **Depends on** | CRUSH-36 Sub-Commit 1 (`9d3c6f0`) — `Frontend: LanguageAdapter` supertrait-tie + Fix #6 blanket. The blanket auto-derives `LanguageAdapter` for any new `impl Frontend`, so this ticket needs only the Frontend impl, not a separate LanguageAdapter impl. |
| **Blocks** | CRUSH-38 (Kotlin walker — shares the Java tree-sitter grammar; this ticket is a hard prereq). Also: CRUSH-39 (walker→AOT for all 12+ walkers — Java is walker #11 of the 12+ tier-1+2 catalog). |
| **Estimated effort** | ~12h work, ~3 commits (Commit 1: skeleton + JavaWalker + JavaFrontend + JavaAdapter + registry insertion + active tests; Commit 2: differential fixtures vs existing Go/C/Zig/Dart walkers + per-extension `can_handle` tests; Commit 3: docs + cross-references). |
| **Branch** | `agent/buffy/M2-JIT-PHASES-2-4` |

## Why this exists

The ROADMAP M6 done conditions name "tier-2 walker expansion" as a primary M6 goal. Java is the natural first tier-2 expansion target because:

1. **Ecosystem coverage** — JVM is the second-largest source-code corpus targeted by Crush today, after the existing 6 (Go/C/Zig/Dart/Wasm) + 6 frontend-language (Bash/Custom/Nepali/Python/Rust/Js) crates, totaling 11. Adding Java makes 12.
2. **Grammar-sharing prereq for Kotlin (CRUSH-38)** — tree-sitter-java is the upstream grammar for tree-sitter-kotlin. Landing Java first unblocks Kotlin with minimal new work (can reuse Java grammar with kotlin extensions).
3. **JVM-runtime path (CRUSH-39)** — Java is the canonical JVM-target language for the planned CRUSH-39 JVM-runtime AOT target. The walker→AOT path benefits from a real JVM corpus to anchor against.

## Scope (Commit 1 — the skeleton, ready to begin immediately)

| File | Change |
|------|--------|
| `crates/crush-walker-core/src/walkers/java.rs` (or alongside Go/C/Zig/Dart) | new file: `pub struct JavaWalker { file_name: String }` + `impl Walker for JavaWalker` (parse/analyze/lower reference impls of the tree-sitter-java grammar) |
| `crates/crush-walker-core/src/lib.rs` | add `mod java;` + add `pub struct JavaFrontend;` + `impl Frontend for JavaFrontend` (auto-derive LanguageAdapter via Fix #6 blanket) |
| `crates/crush-walker-core/src/lib.rs` (AdapterRegistry::with_defaults) | insert `(JavaAdapter, vec![JavaFrontend])` pair into the registry defaults |
| `crates/crush-walker-core/src/adapter.rs` (or sibling tests file) | add `mod java_walker_tests` with `java_walker_registers_in_unified_registry` active test (mirror the Go/C/Zig test pattern) |

## Scope (Commit 2 — differential fixtures + per-extension tests)

| Item | Description |
|------|-------------|
| Differential `<Java>.crush` | Write a small Java source file fixture (~50 lines) annotated with the 5 most common Java patterns (class declaration, method body, imports, generics, lambda). Verify `crush-diff` produces identical AST across all 5 execution tiers (fastvm / jit / aot-rust / aot-c / walker-output). |
| per-extension `can_handle` unit test | Assert `JavaAdapter::can_handle("foo.java") == true` and `JavaAdapter::can_handle("foo.py") == false` (file-extension receive-and-reject case). |

## Scope (Commit 3 — docs + cross-references)

| Item | Description |
|------|-------------|
| Crush frontend `crush_lang_java` analog | Confirm `crush-lang-java` crate or similar path; if not, document the walker-only path. |
| README/java.md | Brief usage notes + supported Java subset + tree-sitter version pin. |
| ROADMAP.md M6 update | Note CRUSH-37 done; bump tier-1+2 walker count to 12. |
| Cross-reference in CRUSH-38 ticket | CRUSH-38 (Kotlin) unblocks now that CRUSH-37 lands. |

## Process note (from CRUSH-36 Sub-Commit 1 lessons)

Per the CRUSH-36 Sub-Commit 1 ticket Process notes (referenced from Commit `9d3c6f0` + Followup commit `32d9f98`):

**Step 0 (mandatory)**: before writing the Java walker, read the existing `mod tests` shape in a sibling walker crate (Go/C/Zig/Dart are good templates). Determine whether the existing crate uses `use super::*;` (glob) or specific imports. **Mirror that style** — if Go uses glob, use glob; if Go uses specific imports, use specific imports.

**Step 1**: rely on the Fix #6 blanket for the new `JavaFrontend`. DO NOT write per-type `impl LanguageAdapter for JavaFrontend` — that would re-trigger E0119 against the blanket. Write `impl LanguageAdapter` only for separate wrapper types (the `<Lang>Adapter` macro pattern) where the wrapper is NOT a Frontend.

**Step 2**: prefer fully-qualified `anyhow::Result<...>` at impl sites over bare `Result<...>` resolvable only via `use anyhow::Result;` scope. This avoids the cross-scope alias-discipline issue that hit Fix #5 (got reverted at Fix #7).

**Step 3** (if Java walker uses `TreeSitterFrontend<JavaWalker>` internally): the free `run_walker_binary` already extends `W: Walker + Send + Sync` from Fix #4. JavaWalker auto-derives Send+Sync as a struct-of-strings; no changes to `run_walker_binary` needed.

## Done condition

- 3 commits land on `agent/buffy/M2-JIT-PHASES-2-4`, each with reviewer LAND-AS-IS verdict and validation GREEN.
- Validation regime: `cargo test -p crush-walker-core --lib` + `cargo check --workspace --tests` (compile-only) + per-FE regression (nepali/bash/custom/python/rust/js) — all GREEN.
- New ticket Status row updates: `Backlog → In Progress → Done`.
- `Hash` row + `Closed by` row populated with final commit SHA when Done.

## Cross-references

- CRUSH-36 (this ticket's prereq): Sub-Commit 1 commit `9d3c6f0` + ticket-update commit `32d9f98`. The cascade closure pattern is documented there.
- CRUSH-38 (this ticket's primary down-stream): Kotlin walker — shares the tree-sitter-java grammar; unblocks immediately upon CRUSH-37 Done.
- CRUSH-39 (this ticket's secondary down-stream): walker→AOT for all 12 walkers — Java walker makes the count 12.
- ROADMAP M6 spec: `.jagent/planning/ROADMAP.md` M6 section.
- Build-order diagram: `.jagent/planning/ROADMAP.md` Build-order section.

## Forward flags

FF1: if a real backend wants to lower Java bytecode to AST instead of using the tree-sitter grammar, that's a CRUSH-37-INT (intermediate) ticket — current scope is grammar-only.

FF2: tree-sitter-java version pin — pin to the same version as the existing Go/C/Zig/Dart walkers for consistent diff-output across the tiers.

FF3: the Commit 2 differential fixture should mirror the `examples/` Java-adjacent fixture (none exists yet — `crush-lang-java` analog may need to be filed separately as CRUSH-37-EX).
## Notes from CRUSH-37 Commit 1 review

**Reviewer verdict** (Nit Pick Nick, final-final): LAND-AS-IS after a 3-fix cascade closed GREEN.

### The 3-fix cascade (architectural lesson)

The Commit 1 source landed in 3 fixes to reach GREEN. The cascade is structurally similar to (but smaller than) Sub-Commit 1's 7-fix cascade. The 3 fixes are:

1. **Fix #1 (`crush-frontend` regular dep)**: the `impl_both_for_walker!` macro hard-references `crush_frontend::language_walkers::LanguageWalker`. With `crush-frontend` as a dev-dep, the macro expansion failed at compile-time. Promoting `crush-frontend` to a regular dep resolved the build failure.

2. **Fix #2 (E0034 ambiguity)**: the test's `frontend_ref.language_name()` was ambiguous because both `Frontend::language_name` and `LanguageAdapter::language_name` are applicable via the Sub-Commit 1 supertrait tie `Frontend: LanguageAdapter`. Disambiguated by using `adapter_ref: &dyn LanguageAdapter` (typed-ref disambiguation).

3. **Fix #3 (extension-format convention)**: the test asserted `adapter_ref.can_handle("java")` but the `TreeSitterFrontend::new` was called with `&[".java"]` (with dot). The `LanguageAdapter::can_handle` default impl is `self.file_extensions().contains(&ext)`, and `AdapterRegistry::walk` extracts the extension via `Path::extension()` which strips the dot. So `can_handle("java")` failed because `file_extensions()` contained `".java"` (with dot) not `"java"`. Resolved by changing `&[".java"]` to `&["java"]` (no dot) in the test.

### The 3-type architecture (mirror of Sub-Commit 1 + Sub-Commit 2 patterns)

- **JavaWalker**: hand-rolled `impl Walker` for tree-sitter-java (mirrors GoWalker). The `language()` method returns `unreachable!()` stub (Commit 2 follow-up).
- **JavaAdapter**: macro-generated ZST via `impl_both_for_walker!(JavaAdapter, ...)`. The macro generates BOTH `impl Walker` (delegating to JavaWalker) AND `impl LanguageWalker` (with tree-sitter parse/walk bridge). NO per-type `impl LanguageAdapter` (per the user's explicit instruction).
- **JavaFrontend**: type alias `pub type JavaFrontend = TreeSitterFrontend<JavaAdapter>;`. The Sub-Commit 1 blanket auto-derives `LanguageAdapter` for `TreeSitterFrontend<JavaAdapter>` (which is a `Frontend` via the existing `impl<W: Walker + Send + Sync> Frontend for TreeSitterFrontend<W>`).

### Cascade closure verification

- JavaAdapter: NO per-type `impl LanguageAdapter` (only macro-generated `impl Walker` + `impl LanguageWalker`). ✓
- JavaFrontend: NO per-type `impl LanguageAdapter` (type alias; Sub-Commit 1 blanket auto-derives). ✓
- The Cascade-closure pattern from Sub-Commit 1 Lesson 4 is applied UP FRONT. ✓

### Latent bug exposed (Go walker crate)

The Java test exposed a latent bug in the existing Go walker crate (`crates/crush-lang-go/src/lib.rs`): the `TreeSitterFrontend::new(walker, "go", &[".go"])` test call uses `&[".go"]` (with dot) but `LanguageAdapter::can_handle("go")` expects `&["go"]` (no dot). The Go test doesn't call `can_handle` so the bug is dormant. This is a forward-flag for a follow-up commit (CRUSH-36 Commit 3 or a Go-specific polish).

### Validation summary

- `crush-lang-java --all-targets`: GREEN
- `crush-lang-java --lib`: 3 tests PASS (Test 1: dual-coercion; Test 2+3 merged: Send/Sync + parse() panic; Test 4: blanket-derive)
- `walker-core --lib`: 9 tests PASS (regression)
- Workspace compile: GREEN
- Per-FE regression (nepali/bash/custom/python/rust/js): all GREEN
- `crush-frontend`: 78 tests PASS (regression)

## Process notes -- the canonical pattern for new walker crates (CRUSH-38 Kotlin, CRUSH-43+)

This commit established the canonical pattern for any new walker crate that uses both `impl_both_for_walker!` (Sub-Commit 2) AND the Sub-Commit 1 blanket. The 3 forward-flag items below MUST be followed by future walker crates to avoid the same cascade.

### F1: `crush-frontend` REGULAR-dep pattern (NEW)

The `impl_both_for_walker!` macro hard-references `crush_frontend::language_walkers::LanguageWalker`. Any walker crate that uses the macro MUST declare `crush-frontend` as a REGULAR dep (not dev-dep). The existing tree-sitter walker crates (Go/C/Zig/Dart) have `crush-frontend` as dev-dep because they use the OLD hand-rolled `impl LanguageAdapter` pattern (no macro reference to `crush-frontend`). The NEW pattern (any walker crate using `impl_both_for_walker!`) requires the regular dep.

**How to apply**: in `crates/crush-lang-{lang}/Cargo.toml`:
```toml
[dependencies]
# ... other deps ...
crush-frontend.workspace = true  # REGULAR dep, not dev-dep
```

### F2: E0034 ambiguity latent risk (Supertrait tie)

The Sub-Commit 1 supertrait tie `Frontend: LanguageAdapter` makes `language_name`/`file_extensions`/`walk` methods ambiguous between the two traits when called via method syntax on `&dyn Frontend`. Use `&dyn LanguageAdapter` (typed-ref disambiguation) or UFCS for these calls.

**How to apply**: in tests, prefer `adapter_ref: &dyn LanguageAdapter` over `frontend_ref: &dyn Frontend` for calls to `language_name()` / `can_handle()`. The `Frontend::walk()` call is unambiguous if invoked via the Frontend trait's default body (which doesn't require `&dyn Frontend`).

### F3: Extension-format convention (no dot)

`LanguageAdapter::can_handle(ext)` checks `self.file_extensions().contains(&ext)`. The default impl of `file_extensions()` returns the extensions stored in the frontend struct. `AdapterRegistry::walk` extracts the extension via `Path::extension()` which strips the dot. So `file_extensions()` must return extensions WITHOUT the leading dot (e.g., `&["java"]` not `&[".java"]`).

**How to apply**: when constructing `TreeSitterFrontend::new(adapter, "lang", &["lang"])` (without dot). The existing Go walker crate uses `&[".go"]` (with dot) — latent bug to be fixed in a follow-up.

### Putting it all together (canonical pattern for new walker crates)

```rust
// crates/crush-lang-{lang}/Cargo.toml
[dependencies]
crush-walker-core.workspace = true
crush-cast.workspace = true
crush-frontend.workspace = true  // F1: regular dep (NOT dev-dep)
tree-sitter.workspace = true
# tree-sitter-{lang} = "0.23"  // CRUSH-37 Commit 2 follow-up

// crates/crush-lang-{lang}/src/lib.rs
use crush_walker_core::{
    impl_both_for_walker, Frontend, LanguageAdapter, TreeSitterFrontend, Walker,
};

pub struct {Lang}Walker { pub file_name: String }

impl Walker for {Lang}Walker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_{lang}::LANGUAGE.into()  // After tree-sitter-{lang} dep
    }
    fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> anyhow::Result<crush_cast::Program> {
        // Real AST -> CAST IR conversion
    }
}

impl_both_for_walker!(
    {Lang}Adapter,
    "{lang}",
    &["{lang-ext}"],  // F3: NO dot
    tree_sitter_{lang}::LANGUAGE.into(),
    {Lang}Walker,
    |fname| {Lang}Walker { file_name: fname }
);

pub type {Lang}Frontend = TreeSitterFrontend<{Lang}Adapter>;
```

### Forward flags for Commit 2 (this ticket)

- **F1**: `tree-sitter-java` workspace dep version pinning (pin to `tree-sitter = "0.25"`-compatible version, matching Go's `tree-sitter-go = "0.23"`).
- **F2**: replace BOTH `unreachable!()` stubs (JavaWalker::language() + macro $ts_lang) with `tree_sitter_java::LANGUAGE.into()`.
- **F3**: real Java AST -> CAST IR conversion (mirror Go's `visit_statement`/`visit_expression` template in `crates/crush-lang-go/src/lib.rs:105-300`).
- **F4**: end-to-end parse test (mirror Go's `test_treesitter_frontend_adapter`).
- **F5**: differential fixtures across the 5 most common Java patterns (class declaration, method body, imports, generics, lambda).
- **F6 (NORM)**: workspace-wide `AdapterRegistry::with_defaults()` that includes Java (CRUSH-37 Commit 1 doesn't add this — Java is registered only in the test's local registry).
