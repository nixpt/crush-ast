//! Java tree-sitter walker (CRUSH-37 Commit 1 skeleton).
//!
//! This walker bridges Java source code into the unified CRUSH
//! trait surface established by CRUSH-36 Commit 2 Sub-Commit 1
//! (`Frontend: LanguageAdapter` supertrait-tie + Fix #6 blanket)
//! and Sub-Commit 2 (`impl_both_for_walker!` macro). The walker
//! applies BOTH patterns cleanly:
//!
//! - **`JavaWalker`** (hand-rolled `impl Walker`): the
//!   tree-sitter-java inner walker. Mirrors Go/C/Zig/Dart. The
//!   `walk()` method converts Java AST to CAST IR.
//!
//! - **`JavaAdapter`** (via `impl_both_for_walker!` macro): the
//!   ZST that registers in BOTH registries. The macro
//!   (Sub-Commit 2, defined in `crates/crush-walker-core/src/lib.rs`)
//!   generates a concrete `impl Walker` that delegates to
//!   `JavaWalker` AND a concrete `impl LanguageWalker` that bridges
//!   parse/walk via tree-sitter. NO per-type
//!   `impl LanguageAdapter for JavaAdapter` is written here --
//!   the Sub-Commit 1 blanket auto-derives `LanguageAdapter` for
//!   the `TreeSitterFrontend<JavaAdapter>` (registered in
//!   `AdapterRegistry`).
//!
//! - **`JavaFrontend`** (a type alias for
//!   `TreeSitterFrontend<JavaAdapter>`): the Sub-Commit 1 pattern
//!   that picks up the blanket auto-derive. Registered in
//!   `AdapterRegistry`.
//!
//! ## CRUSH-37 Commit 1 scope (skeleton)
//!
//! - JavaWalker struct + `impl Walker` (tree-sitter-java stub)
//! - JavaAdapter via the macro (with `unreachable!()` as `$ts_lang`)
//! - JavaFrontend type alias
//! - Active tests (structural-coercion + registry insertion)
//!
//! Real Java parsing requires adding `tree-sitter-java` to the
//! workspace (Commit 2 follow-up). The skeleton uses
//! `unreachable!()` as a `$ts_lang` stub (per Sub-Commit 2's test
//! pattern) -- the type system is satisfied, the parse() path
//! panics at runtime until the real grammar binding is added.
//!
//! ## Forward flags (Commit 2)
//!
//! F1: `tree-sitter-java` workspace dep version pinning (pin to a `tree-sitter = "0.25"`-compatible version, matching Go's `tree-sitter-go = "0.23"`).
//! F2: replace BOTH `unreachable!()` stubs (JavaWalker::language() + macro $ts_lang) with `tree_sitter_java::LANGUAGE.into()`. After F2, the `parse() panic` test should be removed (no longer needed once the real binding is in).
//! F3: real Java AST -> CAST IR conversion (mirror Go's `visit_statement`/`visit_expression` pattern in `crates/crush-lang-go/src/lib.rs`).
//! F4: end-to-end parse test (mirror Go's `test_treesitter_frontend_adapter`).
//! F5: differential fixtures across the 5 most common Java patterns per the CRUSH-37 ticket's Commit 2 scope (class declaration, method body, imports, generics, lambda).

use crush_walker_core::{
    impl_both_for_walker, Frontend, LanguageAdapter, TreeSitterFrontend, Walker,
};
use crush_cast::Program;
use tree_sitter::Tree;

/// Java inner walker. Mirrors Go/C/Zig/Dart.
///
/// `walk()` converts Java AST to CAST IR.
///
/// CRUSH-37 Commit 1 limitation: the `language()` method returns
/// `unreachable!()` as a stub (the real `tree_sitter_java::LANGUAGE.into()`
/// requires the `tree-sitter-java` workspace dep, which is the
/// Commit 2 follow-up). The skeleton compiles + tests but
/// `language()` panics at runtime until the real binding is added.
pub struct JavaWalker {
    pub file_name: String,
}

impl Walker for JavaWalker {
    fn language(&self) -> tree_sitter::Language {
        // CRUSH-37 Commit 1 stub. Replaced with
        // `tree_sitter_java::LANGUAGE.into()` in Commit 2.
        unreachable!(
            "JavaWalker::language() is a stub in CRUSH-37 Commit 1; \
             add tree-sitter-java to the workspace in Commit 2"
        )
    }

    fn walk(&self, _tree: &Tree, _source: &[u8]) -> anyhow::Result<Program> {
        // CRUSH-37 Commit 1 stub. Commit 2 implements real Java
        // AST -> CAST IR conversion (mirrors GoWalker::walk).
        Ok(Program::default())
    }
}

/// JavaAdapter: the macro-generated ZST that registers in BOTH
/// `WalkerRegistry` (via `impl LanguageWalker`) AND can be wrapped
/// in `TreeSitterFrontend<JavaAdapter>` for `AdapterRegistry`
/// (via the Sub-Commit 1 blanket auto-derive for `LanguageAdapter`).
///
/// Per the Sub-Commit 2 macro's cascade-closure pattern: the
/// macro generates CONCRETE impls on a ZST (no supertrait tie, no
/// blanket impl in THIS crate). The Sub-Commit 1 blanket is in
/// walker-core (per the cascade-closure-up-front Sub-Commit 1
/// Lesson 4 pattern) and auto-derives `LanguageAdapter` for
/// `TreeSitterFrontend<JavaAdapter>` when it is registered.
///
/// Notably: there is NO per-type `impl LanguageAdapter for
/// JavaAdapter` here -- per the user's explicit instruction
/// ("do NOT write per-type `impl LanguageAdapter for
/// FrontendType`"). The blanket does the work.
impl_both_for_walker!(
    JavaAdapter,
    "java",                                 // LanguageWalker::language()
    &["java"],                              // LanguageWalker::extensions()
    unreachable!(),                         // Walker::language() -- stub (Commit 1)
    JavaWalker,                             // inner walker type
    |fname| JavaWalker { file_name: fname } // walker ctor
);

/// JavaFrontend: the Sub-Commit 1 pattern for `AdapterRegistry`.
///
/// `TreeSitterFrontend<JavaAdapter>` is a `Frontend` (via the
/// existing `impl<W: Walker + Send + Sync> Frontend for
/// TreeSitterFrontend<W>` in walker-core), and the Sub-Commit 1
/// blanket auto-derives `LanguageAdapter` for it. This is the
/// `AdapterRegistry` integration path for Java.
pub type JavaFrontend = TreeSitterFrontend<JavaAdapter>;

// ── Active tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crush_frontend::language_walkers::{LanguageWalker, WalkerRegistry};
    use crush_walker_core::AdapterRegistry;

    // ── Test 1: Sub-Commit 2 macro-generated dual-coercion ──────────
    //
    // The macro-generated `JavaAdapter` (ZST) must coerce to
    // BOTH `Box<dyn Walker>` AND `Box<dyn LanguageWalker>` from a
    // single value. This is the cascade-closure-up-front proof
    // for the new walker (per Sub-Commit 1 Lesson 4 + Sub-Commit 2
    // macro design).

    #[test]
    fn java_adapter_dual_coercion_structural_proof() {
        // The SAME macro-generated ZST instance.
        let adapter = JavaAdapter;

        // Compile-time + runtime proof: Box<dyn Walker> coerces.
        let walker_box: Box<dyn Walker> = Box::new(adapter);

        // Compile-time + runtime proof: Box<dyn LanguageWalker> coerces.
        let lang_box: Box<dyn LanguageWalker> = Box::new(adapter);

        assert_eq!(lang_box.language(), "java");
        assert_eq!(lang_box.extensions(), &["java"]);

        // Walk registration in WalkerRegistry (Sub-Commit 2 macro surface).
        let mut registry = WalkerRegistry::new();
        registry.register_walker(lang_box);
        let languages = registry.supported_languages();
        assert!(
            languages.contains(&"java".to_string()),
            "JavaAdapter must register in WalkerRegistry; got: {:?}",
            languages
        );

        // Box<dyn Walker> coercion already proven via walker_box;
        // drop for completeness.
        drop(walker_box);
    }

    // ── Test 2 + 3 merged: Send/Sync + parse() panic ─────────────────
    //
    // Per Sub-Commit 2's F8 forward flag: the macro's $ts_lang is
    // `unreachable!()` for the Commit 1 skeleton. parse() panics
    // at runtime. The catch_unwind verifies the structural assembly
    // (the macro generated the parse() body with the unreachable!()
    // token). The Send/Sync check is const-enforced at compile time
    // (macro's const assertion); this is a runtime confirmation.

    #[test]
    fn java_adapter_zst_send_sync_and_parse_panic() {
        // ── Send + Sync at runtime ──────────────────────────────────
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<JavaAdapter>();
        // JavaWalker (struct-with-string) is also auto-Send+Sync.
        assert_send_sync::<JavaWalker>();

        // ── parse() panics on unreachable!() stub language ──────────
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let adapter = JavaAdapter;
        let lang: Box<dyn LanguageWalker> = Box::new(adapter);

        let result = catch_unwind(AssertUnwindSafe(|| {
            lang.parse("dummy source", Some("test.java"))
        }));

        assert!(
            result.is_err(),
            "expected parse() to panic on unreachable!() tree_sitter::Language; got: {:?}",
            result
        );
    }

    // ── Test 4: Sub-Commit 1 blanket auto-derives LanguageAdapter ───
    //
    // `TreeSitterFrontend<JavaAdapter>` is a `Frontend` (via the
    // existing impl), and the Sub-Commit 1 blanket auto-derives
    // `LanguageAdapter` for it. This is the `AdapterRegistry`
    // integration path -- the blanket does the work, NOT a per-type
    // `impl LanguageAdapter for JavaAdapter` (per the user's
    // explicit instruction).

    #[test]
    fn java_frontend_via_subcommit1_blanket_registers_in_adapter_registry() {
        // Construct TreeSitterFrontend<JavaAdapter>.
        // NOTE: extensions MUST be WITHOUT the leading dot (e.g.,
        // `&["java"]` not `&[".java"]`). The `LanguageAdapter::can_handle`
        // default impl is `self.file_extensions().contains(&ext)`,
        // and `AdapterRegistry::walk` extracts the extension via
        // `Path::extension()` which strips the dot. Using `&[".java"]`
        // would mismatch `can_handle("java")` and the test would
        // fail. The existing Go walker crate uses `&[".go"]` (with
        // dot) in its TreeSitterFrontend::new call but doesn't call
        // `can_handle` in its tests -- this is a latent inconsistency
        // that the Java test exposes.
        let frontend = JavaFrontend::new(JavaAdapter, "java", &["java"]);

        // Compile-time check: TreeSitterFrontend<JavaAdapter> is a Frontend.
        // NOTE: do NOT call `frontend_ref.language_name()` -- both
        // `Frontend` and `LanguageAdapter` (via the Sub-Commit 1
        // supertrait tie `Frontend: LanguageAdapter`) define a
        // `language_name()` method, so the call would be ambiguous
        // (E0034). Use `adapter_ref` (typed as `&dyn LanguageAdapter`)
        // for compiler-disambiguated calls.
        let _frontend_ref: &dyn Frontend = &frontend;

        // Compile-time check: Sub-Commit 1 blanket auto-derives
        // LanguageAdapter for Frontend. No per-type impl was written.
        let adapter_ref: &dyn LanguageAdapter = &frontend;

        // Register in AdapterRegistry (the unified registry).
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(JavaFrontend::new(
            JavaAdapter,
            "java",
            &["java"],
        )));

        let languages = registry.languages();
        assert!(
            languages.contains(&"java"),
            "JavaFrontend must register in AdapterRegistry via Sub-Commit 1 blanket; got: {:?}",
            languages
        );

        // Sanity refs via `adapter_ref` (compiler-disambiguated).
        assert_eq!(adapter_ref.language_name(), "java");
        assert!(adapter_ref.can_handle("java"));
        assert!(!adapter_ref.can_handle("py"));
    }
}
