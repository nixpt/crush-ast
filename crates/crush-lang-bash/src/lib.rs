pub mod analyzer;
pub mod lowerer;
pub mod parser;

use crush_cast::Program;
use std::any::Any;
use walker_core::{FeatureReport, Frontend, LowerCtx};

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
    let (_, program) = walker_core::frontend_pipeline(&BashFrontend, source)?;
    Ok(program)
}
