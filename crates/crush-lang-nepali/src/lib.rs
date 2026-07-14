use crush_cast::Program;
use std::any::Any;
use walker_core::{FeatureReport, Frontend};
use crush_frontend::parse_source;

pub struct NepaliFrontend;

impl Frontend for NepaliFrontend {
    fn language_name(&self) -> &'static str {
        "nepcode"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &[".np", ".nepali"]
    }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        // Our updated lexer natively parses Nepali keywords into standard AST.
        let mut program = parse_source(source)?;
        program.lang = Some("nepcode".to_string());
        Ok(Box::new(program))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let program = ast
            .downcast_ref::<Program>()
            .ok_or_else(|| anyhow::anyhow!("expected Program"))?;
        
        let mut report = FeatureReport::default();
        report.lang = "nepcode".to_string();
        report.uses_functions = !program.functions.is_empty();
        report.estimated_complexity = program.functions.len();
        Ok(report)
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let program = *ast
            .downcast::<Program>()
            .map_err(|_| anyhow::anyhow!("expected Program"))?;
        Ok(program)
    }
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use walker_core::impl_adapter_from_frontend;

pub fn nepali_to_cast(source: &str) -> anyhow::Result<crush_cast::Program> {
    let frontend = crate::NepaliFrontend;
    let (_, program) = walker_core::frontend_pipeline(&frontend, source)?;
    Ok(program)
}

impl_adapter_from_frontend!(
    NepcodeAdapter,
    "nepcode",
    &["np", "nepali"],
    crate::nepali_to_cast
);
pub mod sdk;
