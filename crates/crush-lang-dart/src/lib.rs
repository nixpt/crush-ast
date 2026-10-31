//! Dart walker — tree-sitter-dart parser → CAST IR.
//!
//! This is a minimal walker proving the LanguageAdapter pattern:
//! adding a new language took ~30 lines of code.

use anyhow::Result;
use crush_cast::Program;
use crush_walker_core::Walker;

pub struct DartWalker {
    pub file_name: String,
}

impl Walker for DartWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_dart::LANGUAGE.into()
    }

    fn walk(&self, _tree: &tree_sitter::Tree, _source: &[u8]) -> Result<Program> {
        // This walker is a stub: it used to discard every node and return an
        // empty program, which then "ran" successfully with no output
        // (CRUSH-203). Refuse loudly instead.
        anyhow::bail!("dart: the Crush Dart walker is not implemented (CRUSH-203)")
    }
}

// ���─ Adapter ───────────────────────────────────────────────────────────────────

use crush_walker_core::LanguageAdapter;

pub struct DartAdapter;

impl LanguageAdapter for DartAdapter {
    fn language_name(&self) -> &'static str { "dart" }
    fn file_extensions(&self) -> &[&'static str] { &["dart"] }
    fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(crush_walker_core::FeatureReport, crush_cast::Program)> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_dart::LANGUAGE.into())
            .map_err(|e| anyhow::anyhow!("tree-sitter-dart init: {e}"))?;
        let tree = parser.parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Dart parse failed"))?;
        let walker = DartWalker { file_name: filename.to_string() };
        let program = walker.walk(&tree, source.as_bytes())?;
        Ok((crush_walker_core::FeatureReport { lang: "dart".to_string(), ..Default::default() }, program))
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
    use crush_walker_core::AdapterRegistry;

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
    fn test_dart_walk_fails_loudly() {
        // CRUSH-203: a stub walker must not return an empty program.
        let src = "void main() { var x = 42; }";
        let err = run_dart(src).unwrap_err().to_string();
        assert!(err.contains("not implemented"), "dart must fail loudly, got: {err}");
    }

    #[test]
    fn test_dart_adapter_in_registry() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(DartAdapter));
        let err = registry.walk("void main() {}", "test.dart").unwrap_err().to_string();
        assert!(err.contains("not implemented"), "dart must fail loudly, got: {err}");
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
