pub mod analyzer;
pub mod lowerer;
pub mod parser;

use crush_cast::Program;
use std::any::Any;
use walker_core::{FeatureReport, Frontend, LowerCtx};

pub struct ZshFrontend;

impl Frontend for ZshFrontend {
    fn language_name(&self) -> &'static str {
        "zsh"
    }
    fn file_extensions(&self) -> &[&'static str] {
        &[".zsh"]
    }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        let program = parser::parse_source(source)?;
        Ok(Box::new((source.to_string(), program)))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let (_, program) = ast
            .downcast_ref::<(String, zshrs_parse::parser::ZshProgram)>()
            .ok_or_else(|| anyhow::anyhow!("expected (String, ZshProgram)"))?;
        Ok(analyzer::analyze_program(program))
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let (source, program) = *ast
            .downcast::<(String, zshrs_parse::parser::ZshProgram)>()
            .map_err(|_| anyhow::anyhow!("expected (String, ZshProgram)"))?;
        let ctx = LowerCtx::new(&source, "<crush>", "zsh");
        lowerer::lower_program(&program, &ctx)
    }
}

pub fn zsh_to_cast(source: &str) -> anyhow::Result<Program> {
    let (_, program) = walker_core::frontend_pipeline(&ZshFrontend, source)?;
    Ok(program)
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use walker_core::impl_adapter_from_frontend;

impl_adapter_from_frontend!(
    ZshAdapter,
    "zsh",
    &["zsh"],
    crate::zsh_to_cast
);
