pub mod analyzer;
pub mod lowerer;
pub mod parser;

use crush_cast::Program;
use std::any::Any;
use crush_walker_core::{FeatureReport, Frontend, LowerCtx};

pub struct BashFrontend;

impl Frontend for BashFrontend {
    fn language_name(&self) -> &'static str {
        "bash"
    }
    fn file_extensions(&self) -> &[&'static str] {
        &[".sh", ".bash"]
    }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        let program = parser::parse_source(source)?;
        Ok(Box::new((source.to_string(), program)))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let (_, program) = ast
            .downcast_ref::<(String, brush_parser::ast::Program)>()
            .ok_or_else(|| anyhow::anyhow!("expected (String, Program)"))?;
        Ok(analyzer::analyze_program(program))
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let (source, program) = *ast
            .downcast::<(String, brush_parser::ast::Program)>()
            .map_err(|_| anyhow::anyhow!("expected (String, Program)"))?;
        let ctx = LowerCtx::new(&source, "<crush>", "bash");
        lowerer::lower_program(program, &ctx)
    }
}

/// Parse bash source and lower to CAST (convenience wrapper).
pub fn bash_to_cast(source: &str) -> anyhow::Result<Program> {
    let (_, program) = crush_walker_core::frontend_pipeline(&BashFrontend, source)?;
    Ok(program)
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use crush_walker_core::impl_adapter_from_frontend;

impl_adapter_from_frontend!(
    BashAdapter,
    "bash",
    &["sh", "bash"],
    crate::bash_to_cast
);

#[cfg(test)]
mod tests {
    //! CRUSH-36 Commit 1: regression-resistance for the Frontend ->
    //! LanguageAdapter migration (already landed via the macro above).
    //! Without this test, the `impl_adapter_from_frontend!` line could be
    //! deleted and the crate would still compile against `Frontend` alone,
    //! silently losing its unified-registry registration.
    use super::*;
    use crush_walker_core::{AdapterRegistry, LanguageAdapter};

    #[test]
    fn bash_adapter_registers_in_unified_registry() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(BashAdapter));
        let langs = registry.languages();
        assert!(
            langs.contains(&"bash"),
            "BashAdapter must register with name 'bash', got: {:?}",
            langs
        );
    }
}

pub mod sdk;
