# CRUSH-37 — Java tree-sitter walker (M6 tier-2 expansion)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-37 |
| **Title** | Java tree-sitter walker + JavaAdapter registry entry (M6 first new tier-2 walker) |
| **Hash** | (to be populated when in-progress work commits) |
| **Status** | Backlog — ready to begin after CRUSH-36 Sub-Commit 1 (`9d3c6f0`) + ticket-update commit (`32d9f98`) |
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
