pub mod parser;
pub mod types;
pub mod semantics;
pub mod optimizer;
pub mod compiler;
pub mod render;
pub mod import_system;

use anyhow::Result;

pub fn compile_crush_source(source: &str) -> Result<casm::Program> {
    let program = parser::Parser::parse(source)
        .map_err(|errors| anyhow::anyhow!("Parse errors: {}", errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")))?;
    let mut sema = semantics::SemanticAnalyzer::new();
    sema.check(&program)?;
    let mut program = program;
    optimizer::Optimizer::optimize(&mut program);
    let mut comp = compiler::Compiler::new();
    comp.compile(program)
}
