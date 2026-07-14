//! Dart walker — tree-sitter-dart parser → CAST IR.
//!
//! This is a minimal walker proving the LanguageAdapter pattern:
//! adding a new language took ~30 lines of code.

use anyhow::Result;
use crush_cast::{self as ast, Program};
use std::collections::HashMap;
use walker_core::{BaseWalker, Walker};

pub struct DartWalker {
    pub file_name: String,
}

impl Walker for DartWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_dart::LANGUAGE.into()
    }

    fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Result<Program> {
        let base = BaseWalker::new(source);
        let root = tree.root_node();
        let mut functions = HashMap::new();
        let mut main_body = Vec::new();

        // Minimal walk: treat top-level nodes as main body stmts.
        // A full walker would recurse into class_declaration, function_signature, etc.
        for child in root.children(&mut root.walk()) {
            let _meta = base.create_meta(child, "dart", &self.file_name);
            // Stub: real walker would visit child nodes and produce CAST Statements
            let _ = child;
        }

        if !main_body.is_empty() {
            functions.entry("main".to_string())
                .or_insert_with(|| ast::Function { params: vec![], body: Vec::new(), meta: HashMap::new(), ..Default::default() })
                .body.extend(main_body);
        }

        Ok(Program {
            cast_version: "0.2".to_string(),
            entry: "main".to_string(),
            lang: Some("dart".to_string()),
            functions,
            ai_meta: None,
            ..Default::default()
        })
    }
}

// ���─ Adapter ───────────────────────────────────────────────────────────────────

use walker_core::LanguageAdapter;

pub struct DartAdapter;

impl LanguageAdapter for DartAdapter {
    fn language_name(&self) -> &'static str { "dart" }
    fn file_extensions(&self) -> &[&'static str] { &["dart"] }
    fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(walker_core::FeatureReport, crush_cast::Program)> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_dart::LANGUAGE.into())
            .map_err(|e| anyhow::anyhow!("tree-sitter-dart init: {e}"))?;
        let tree = parser.parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Dart parse failed"))?;
        let walker = DartWalker { file_name: filename.to_string() };
        let program = walker.walk(&tree, source.as_bytes())?;
        Ok((walker_core::FeatureReport { lang: "dart".to_string(), ..Default::default() }, program))
    }
}

// ── Binary (for subprocess dispatch) ──────────────────────────────────────────

pub fn dart_to_cast(source: &str, filename: &str) -> anyhow::Result<Program> {
    let adapter = DartAdapter;
    let (_, program) = adapter.walk(source, filename)?;
    Ok(program)
}

// ── SDK ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod sdk {
    use super::*;
    use walker_core::AdapterRegistry;

    pub fn run_dart(source: &str) -> anyhow::Result<String> {
        let adapter = DartAdapter;
        let (_, cast) = adapter.walk(source, "test.dart")
            .map_err(|e| anyhow::anyhow!("dart->CAST: {e}"))?;
        let mut compiler = crush_frontend::compiler::Compiler::new();
        let casm = compiler.compile(cast).map_err(|e| anyhow::anyhow!("CAST->CASM: {e}"))?;
        let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm)
            .map_err(|e| anyhow::anyhow!("CASM->CVM1: {e}"))?;
        let quotas = crush_vm::vm::Quotas { max_steps: 10_000_000, ..Default::default() };
        let result = crush_vm::vm::run_with_caps(&vm_prog, &quotas, None)
            .map_err(|e| anyhow::anyhow!("CVM1: {e}"))?;
        Ok(result.output.trim().to_string())
    }

    #[test]
    fn test_dart_walk_no_crash() {
        let src = "void main() { var x = 42; }";
        let r = run_dart(src);
        assert!(r.is_ok(), "dart walk should not crash: {r:?}");
    }

    #[test]
    fn test_dart_adapter_in_registry() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(DartAdapter));
        let result = registry.walk("void main() {}", "test.dart");
        assert!(result.is_ok(), "dart adapter should walk via registry: {result:?}");
    }

    #[test]
    fn test_dart_adapter_extensions() {
        let adapter = DartAdapter;
        assert!(adapter.can_handle("dart"));
        assert!(!adapter.can_handle("js"));
        assert_eq!(adapter.language_name(), "dart");
        assert_eq!(adapter.file_extensions(), &["dart"]);
    }
}
