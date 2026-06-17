pub mod ai_runtime;
pub mod compiler;
pub mod import_system;
pub mod language_walkers;
pub mod optimizer;
pub mod parser;
pub mod polyglot_imports;
pub mod render;
pub mod semantics;
pub mod types;

use anyhow::Result;

/// Parse Crush source code into a CAST Program.
pub fn parse_source(source: &str) -> Result<crush_cast::Program> {
    parser::Parser::parse(source).map_err(|errors| {
        anyhow::anyhow!(
            "Parse errors: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// Compile an already-parsed CAST Program into CASM bytecode.
///
/// Runs semantic analysis, optimization, and code generation.
pub fn compile_cast(program: &crush_cast::Program) -> Result<casm::Program> {
    let mut program = program.clone();
    let mut sema = semantics::SemanticAnalyzer::new();
    sema.check(&program)?;
    optimizer::Optimizer::optimize(&mut program);
    let mut comp = compiler::Compiler::new();
    comp.compile(program)
}

/// Parse Crush source and compile directly to CASM bytecode.
pub fn compile_crush_source(source: &str) -> Result<casm::Program> {
    let program = parse_source(source)?;
    compile_cast(&program)
}
