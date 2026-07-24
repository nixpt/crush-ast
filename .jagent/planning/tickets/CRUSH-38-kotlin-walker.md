# CRUSH-38 — Kotlin tree-sitter walker (M6 tier-2 expansion)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-38 |
| **Title** | Kotlin tree-sitter walker + KotlinAdapter registry entry (M6 second tier-2 walker; shares tree-sitter-java grammar with CRUSH-37) |
| **Hash** | (to be populated when in-progress work commits) |
| **Status** | Backlog — ready to begin after CRUSH-37 Commit 1 (`9708954`) + Cargo.lock chore (`d2cda40`) lands the Java skeleton; CRUSH-37 Commit 2 (real Java parsing) is NOT a hard prereq because Kotlin can use `tree-sitter-kotlin` (a separate crate) directly without depending on the Java parser. |
| **Phase** | M6 — Walker parity & multi-language completeness |
| **Assignee** | Buffy |
| **Depends on** | CRUSH-36 Sub-Commit 1 (`9d3c6f0`) — `Frontend: LanguageAdapter` supertrait-tie + Fix #6 blanket. CRUSH-37 Commit 1 (`9708954`) — the canonical pattern (3-type architecture + F1/F2/F3 process notes) for any new walker crate. |
| **Blocks** | CRUSH-39 (walker→AOT for all 12+ walkers — Kotlin makes the count 13). Also: CRUSH-SCALA (Scala walker, dialect of Kotlin on JVM; same grammar family). |
| **Estimated effort** | ~10h work, ~3 commits (Commit 1 skeleton + KotlinWalker + KotlinAdapter + KotlinFrontend + 3 active tests + workspace registration; Commit 2 real Kotlin AST → CAST IR + end-to-end test + 5 differential fixtures; Commit 3 docs + cross-references). |
| **Branch** | `agent/buffy/M2-JIT-PHASES-2-4` |

## Why this exists

The ROADMAP M6 done conditions name "tier-2 walker expansion" as a primary M6 goal. Kotlin is the natural second tier-2 expansion target because:

1. **Ecosystem coverage** — Kotlin is the second-largest JVM language after Java. Adding Kotlin makes the high-value JVM corpus (Java + Kotlin) fully representable in CRUSH.
2. **Grammar-sharing with Java (CRUSH-37)** — `tree-sitter-kotlin` is a fork/extension of `tree-sitter-java`. Both grammars share the tree-sitter-java dep tree. The Kotlin walker uses `tree-sitter-kotlin` (a separate crate on crates.io) for the actual grammar, but the binary dependencies are minimal.
3. **JVM-runtime path (CRUSH-39)** — Kotlin is the canonical JVM-target language for the planned CRUSH-39 JVM-runtime AOT target. The walker→AOT path benefits from a real Kotlin corpus to anchor against.
4. **Pre-step-pattern continuation** — establishing that the CRUSH-37 canonical pattern (3-type architecture + F1/F2/F3 process notes) generalizes to NEW walker crates. This is the structural template test for the M6 walker expansion pattern.

## Scope (Commit 1 — the skeleton, ready to begin immediately)

Mirror CRUSH-37 Commit 1 (`9708954`) — the canonical 3-type architecture (KotlinWalker + KotlinAdapter + KotlinFrontend) + 3 active tests + workspace registration.

| File | Change |
|------|--------|
| `crates/crush-lang-kotlin/Cargo.toml` (new file) | mirror CRUSH-37's Cargo.toml; `crush-frontend` is a REGULAR dep per F1 (the macro hard-references `crush_frontend::language_walkers::LanguageWalker`). `tree-sitter-kotlin` is NOT yet added (Commit 2 follow-up, similar to CRUSH-37's Commit 1 `tree-sitter-java` stub). |
| `crates/crush-lang-kotlin/src/lib.rs` (new file) | 3-type architecture: `KotlinWalker` (hand-rolled `impl Walker` with `unreachable!()` `language()` stub + `Ok(Program::default())` `walk()` stub) + `KotlinAdapter` (via `impl_both_for_walker!` macro with `unreachable!()` `$ts_lang` stub) + `KotlinFrontend` (type alias `pub type KotlinFrontend = TreeSitterFrontend<KotlinAdapter>;`). 3 active tests: dual-coercion + Send/Sync+parse-panic + blanket-derive. |
| `crates/crush-lang-kotlin/src/main.rs` (new file) | mirror CRUSH-37's main.rs; binary stub calls `run_walker_binary(KotlinWalker { file_name: cli.input.clone() }, "kotlin", &["kotlin"], &cli.input)`. Will panic at runtime on parse() due to `unreachable!()` stub language (Commit 2 limitation). |
| `Cargo.toml` (workspace) | add `"crates/crush-lang-kotlin"` to the members list, after `crush-lang-java`. |

### Sub-step 0 (mandatory, per CRUSH-37's Process note Step 0)

Before writing the Kotlin walker, read the existing `mod tests` shape in `crates/crush-lang-java/src/lib.rs` (the just-merged CRUSH-37 Commit 1). Determine whether the existing crate uses `use super::*;` (glob) or specific imports.

**Observed shape (from CRUSH-37 Commit 1)**: `mod tests { use super::*;` (the imports are at the top of the test mod as `use super::*; use crush_frontend::language_walkers::{LanguageWalker, WalkerRegistry}; use crush_walker_core::AdapterRegistry;`). The macro-generated `JavaAdapter` is in `super::*` scope via `use super::*;`. This is the GLOB pattern.

**Mirror that style in Kotlin**: use `use super::*;` at the top of the test mod.

### Sub-step 1 (mandatory, per CRUSH-37 F1)

`crush-frontend` MUST be a REGULAR dep in `crates/crush-lang-kotlin/Cargo.toml`. The `impl_both_for_walker!` macro hard-references `crush_frontend::language_walkers::LanguageWalker`. With `crush-frontend` as a dev-dep, the macro expansion fails at compile-time.

```toml
[dependencies]
crush-walker-core.workspace = true
crush-cast.workspace = true
crush-frontend.workspace = true  # F1: REGULAR dep (NOT dev-dep)
tree-sitter.workspace = true
# tree-sitter-kotlin = "0.3"  # TODO: Commit 2 follow-up
```

### Sub-step 2 (mandatory, per CRUSH-37 F2)

For the E0034 ambiguity latent risk (Supertrait tie `Frontend: LanguageAdapter` makes `language_name`/`file_extensions`/`walk` methods ambiguous on `&dyn Frontend`), use `&dyn LanguageAdapter` (typed-ref disambiguation) for `language_name()` / `can_handle()` calls in tests. DO NOT use `frontend_ref.language_name()` — that triggers E0034 because both `Frontend::language_name` and `LanguageAdapter::language_name` are applicable via the supertrait tie.

### Sub-step 3 (mandatory, per CRUSH-37 F3)

Use NO-DOT extension format: `TreeSitterFrontend::new(adapter, "kotlin", &["kotlin"])` (not `&[".kotlin"]`). The `LanguageAdapter::can_handle` default impl uses `Path::extension()`-extracted (no-dot) extensions, so `&[".kotlin"]` would make `can_handle("kotlin")` return `false`. The existing Go walker crate has a latent bug (`&[".go"]` with dot); Kotlin should NOT inherit that bug.

## Scope (Commit 2 — real Kotlin parsing + differential fixtures)

| Item | Description |
|------|-------------|
| `tree-sitter-kotlin` workspace dep | Add `tree-sitter-kotlin = "0.3"` (or compatible version) to `Cargo.toml` workspace.dependencies + `crates/crush-lang-kotlin/Cargo.toml`. |
| Replace BOTH `unreachable!()` stubs | `KotlinWalker::language()` + macro `$ts_lang` → `tree_sitter_kotlin::LANGUAGE.into()` (mirror CRUSH-37 Commit 2 F2). |
| Real Kotlin AST → CAST IR conversion | Implement `KotlinWalker::walk()` mirroring Go's `visit_statement`/`visit_expression` template (`crates/crush-lang-go/src/lib.rs:105-300`). The Kotlin AST node kinds are similar to Java but with Kotlin-specific constructs (data classes, sealed classes, extension functions, `object` expressions, coroutines). |
| End-to-end parse test | Mirror Go's `test_treesitter_frontend_adapter` style: parse a Kotlin `hello.kt` source, assert `(report, program)` tuple is correct. |
| 5 differential fixtures | Tests across the 5 most common Kotlin patterns: data class declaration, top-level function with extension receiver, sealed class hierarchy, `object` expression, lambda with receiver. Verify `crush-diff` produces identical AST across all 5 execution tiers (fastvm / jit / aot-rust / aot-c / walker-output). |
| Remove parse() panic test | Mirror CRUSH-37 Commit 2: once the `$ts_lang` is real, the panic test is no longer needed. Remove the `parse_panics_on_unreachable_dummy_lang` test (or replace with a successful parse test). |

## Scope (Commit 3 — docs + cross-references)

| Item | Description |
|------|-------------|
| `README/kotlin.md` | Brief usage notes + supported Kotlin subset + tree-sitter version pin. |
| ROADMAP.md M6 update | Note CRUSH-38 done; bump tier-1+2 walker count to 13. |
| Cross-reference in CRUSH-39 ticket | CRUSH-39 (walker→AOT for all 12+ walkers) unblocks now that Java + Kotlin are in. |
| Cross-reference in CRUSH-SCALA ticket (new) | CRUSH-38 (Kotlin) unblocks potential CRUSH-SCALA (Scala walker, JVM dialect). Note: Scala is a separate ticket — file when CRUSH-38 is done. |
| Documentation cross-link | Reference CRUSH-37's F1/F2/F3 process notes as the canonical pattern for future walker crates (CRUSH-SCALA, CRUSH-43+ tier-3 like Swift, Kotlin, Erlang/Elixir). |

## Process notes (from CRUSH-37 Commit 1 review)

This ticket inherits CRUSH-37's 3-fix cascade closure pattern. The 3 forward-flag items below MUST be followed to avoid the same cascade:

### F1: `crush-frontend` REGULAR-dep pattern (cross-references CRUSH-37's F1)

Per CRUSH-37's Process note F1: `impl_both_for_walker!` macro hard-references `crush_frontend::language_walkers::LanguageWalker`. Any walker crate using the macro MUST declare `crush-frontend` as a REGULAR dep. Kotlin's `Cargo.toml` MUST follow this pattern.

### F2: E0034 ambiguity latent risk (cross-references CRUSH-37's F2)

Per CRUSH-37's Process note F2: the Sub-Commit 1 supertrait tie `Frontend: LanguageAdapter` makes `language_name`/`file_extensions`/`walk` methods ambiguous on `&dyn Frontend`. Use `&dyn LanguageAdapter` or UFCS for these calls. CRUSH-37 test 4 demonstrates this pattern; Kotlin's test 4 (the blanket-derive test) MUST follow the same pattern.

### F3: Extension-format convention (cross-references CRUSH-37's F3)

Per CRUSH-37's Process note F3: `LanguageAdapter::can_handle` uses `Path::extension()`-extracted (no-dot) extensions. Use `&["kotlin"]` (no dot), not `&[".kotlin"]`. The Java test exposes a latent bug in the Go walker crate (`&[".go"]` with dot); Kotlin should NOT inherit that bug.

### Pre-step (cross-references CRUSH-37's Process note Step 0)

Read existing `mod tests` shape in `crates/crush-lang-java/src/lib.rs` BEFORE writing the Kotlin test mod. Mirror the glob pattern (`use super::*;` at the top of the test mod). If the Java test mod were to use specific imports, Kotlin would mirror that.

## Done condition

- 3 commits land on `agent/buffy/M2-JIT-PHASES-2-4`, each with reviewer LAND-AS-IS verdict and validation GREEN.
- Validation regime: `cargo test -p crush-lang-kotlin --lib` + `cargo check --workspace --tests` (compile-only) + per-FE regression (nepali/bash/custom/python/rust/js/java) — all GREEN.
- New ticket Status row updates: `Backlog → In Progress → Done`.
- `Hash` row + `Closed by` row populated with final commit SHA when Done.
- CRUSH-37's F1/F2/F3 process notes are followed (avoid the 3-fix cascade).

## Cross-references

- CRUSH-37 (this ticket's prereq): Commit 1 (`9708954`) + Cargo.lock chore (`d2cda40`). The canonical pattern (3-type architecture + F1/F2/F3 process notes) is documented there.
- CRUSH-36 (this ticket's transitive prereq): Sub-Commit 1 (`9d3c6f0`) + ticket-update commit (`32d9f98`) + Sub-Commit 2 (`9593da6`) + tickets `3530dcb` and `54b9b0e`. The cascade closure pattern is documented there.
- CRUSH-39 (this ticket's secondary down-stream): walker→AOT for all 12+ walkers — Kotlin walker makes the count 13.
- CRUSH-SCALA (this ticket's potential dialect): Scala walker (future ticket, depends on CRUSH-38 finishing).
- ROADMAP M6 spec: `.jagent/planning/ROADMAP.md` M6 section.
- Build-order diagram: `.jagent/planning/ROADMAP.md` Build-order section.

## Forward flags

FF1: if Kotlin's coroutine support is needed (CRUSH-38-COROUTINES scope), the differential fixtures need to include `suspend fun` + `Flow` + `launch` patterns. Current scope is grammar-only — coroutines are a follow-up.

FF2: tree-sitter-kotlin version pin — pin to a `tree-sitter = "0.25"`-compatible version. Verify compatibility with the existing `tree-sitter-java` dep tree (shared dep).

FF3: data class + sealed class + object expression + extension functions are the 4 most Kotlin-specific constructs. Include them in the Commit 2 differential fixtures.

FF4: the existing Go walker crate's latent `&[".go"]` bug could be fixed in parallel with CRUSH-38 (a separate polish commit). The Kotlin walker should NOT inherit that bug.

FF5: per CRUSH-37's F1/F2/F3 pattern, any future walker crate files (CRUSH-SCALA, CRUSH-43+ tier-3 like Swift, Kotlin, Erlang/Elixir) MUST follow the same 3-type architecture + F1/F2/F3 process notes. The Kotlin ticket is the second test of this pattern.

## Decomposition summary (3 commits)

| Commit | Scope | Validation | Cascade-risk |
|--------|-------|------------|--------------|
| 1 | Skeleton: KotlinWalker + KotlinAdapter (macro) + KotlinFrontend (type alias) + 3 active tests + workspace registration | `cargo test -p crush-lang-kotlin --lib` + `cargo check --workspace --tests` + per-FE regression | 3-fix cascade (per CRUSH-37 pattern) — mitigated by F1/F2/F3 process notes |
| 2 | Real Kotlin parsing: `tree-sitter-kotlin` dep + replace unreachable!() stubs + real AST → CAST IR conversion + end-to-end test + 5 differential fixtures | `cargo test -p crush-lang-kotlin --lib` (end-to-end) + `crush-diff` cross-tier test | E0034 latent risk (F2) — mitigated by typed-ref disambiguation |
| 3 | Docs: README/kotlin.md + ROADMAP M6 update + cross-references | `cargo check --workspace --tests` (no functional change) | None — markdown-only |
