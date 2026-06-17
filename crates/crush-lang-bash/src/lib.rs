pub mod analyzer;
pub mod lowerer;
pub mod parser;

use std::any::Any;
use crush_cast::Program;
use walker_core::{FeatureReport, Frontend};

pub struct BashFrontend;

impl Frontend for BashFrontend {
    fn language_name(&self) -> &'static str { "bash" }
    fn file_extensions(&self) -> &[&'static str] { &[".sh", ".bash"] }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        let program = parser::parse_source(source)?;
        Ok(Box::new(program))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let program = ast.downcast_ref::<brush_parser::ast::Program>()
            .ok_or_else(|| anyhow::anyhow!("expected brush-parser Program"))?;
        Ok(analyzer::analyze_program(program))
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let program = ast.downcast::<brush_parser::ast::Program>()
            .map_err(|_| anyhow::anyhow!("expected brush-parser Program"))?;
        lowerer::lower_program(*program)
    }
}

/// Parse bash source and lower to CAST (convenience wrapper).
pub fn bash_to_cast(source: &str) -> anyhow::Result<Program> {
    let (_, program) = walker_core::frontend_pipeline(&BashFrontend, source)?;
    Ok(program)
}
