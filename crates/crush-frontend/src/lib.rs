pub mod ai_runtime;
pub mod cast_enrich;
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

/// Parse Crush source code into an enriched CAST Program.
///
/// Runs the parser then the CAST enrichment pass which populates
/// `Program.exhaustive_sites` from match expressions found in function bodies.
/// Annotations from `@module`, `@invariant`, `@errors` etc. are already in
/// the CAST from the parser; enrichment adds derived fields that require
/// walking the full program tree.
pub fn parse_source(source: &str) -> Result<crush_cast::Program> {
    let mut program = parser::Parser::parse(source).map_err(|errors| {
        anyhow::anyhow!(
            "Parse errors: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    cast_enrich::enrich_cast(&mut program);
    Ok(program)
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

// ── Surgical symbol extraction ───────────────────────────────────────────────
//
// These functions solve the "1000-line file" problem: instead of reading an
// entire Crush source file to understand one function, callers extract just
// the symbol they need — with its annotations rendered inline.

/// Description of a top-level symbol in a Crush source file.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Symbol name (function or struct name).
    pub name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// One-line purpose from `@module.exports` or from `@errors` annotation if present.
    pub annotation_summary: String,
}

/// Kind of a top-level Crush symbol.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Struct,
}

/// List all top-level symbols in a Crush source file.
///
/// Returns `(name, kind, annotation_summary)` tuples sorted by name so an
/// agent can do a cheap "what's in this file?" scan without reading bodies.
pub fn list_symbols(source: &str) -> Result<Vec<SymbolInfo>> {
    let program = parse_source(source)?;
    let mut symbols: Vec<SymbolInfo> = Vec::new();

    for (name, func) in &program.functions {
        if name == "main" {
            continue;
        }
        let summary = func
            .annotations
            .as_ref()
            .map(|a| {
                if !a.errors.is_empty() {
                    format!("errors: {}", a.errors.join(", "))
                } else if !a.covers.is_empty() {
                    format!("covers: {}", a.covers.join(", "))
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
        symbols.push(SymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Function,
            annotation_summary: summary,
        });
    }

    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(symbols)
}

/// Extract a single named symbol from Crush source and render it back as source.
///
/// Includes any `@errors`, `@reads`, `@writes`, `@covers` etc. annotations so
/// the caller gets the full contract alongside the implementation.
///
/// Returns `Err` if the source fails to parse or the symbol is not found.
pub fn extract_symbol(source: &str, name: &str) -> Result<String> {
    let program = parse_source(source)?;
    if let Some(func) = program.functions.get(name) {
        return Ok(render::render_function_standalone(name, func));
    }
    anyhow::bail!("symbol '{}' not found in source", name)
}

/// Extract the module-level manifest (`@module { ... }`) from Crush source.
///
/// Returns `None` if no `@module` annotation was declared.
pub fn extract_manifest(source: &str) -> Result<Option<String>> {
    let program = parse_source(source)?;
    Ok(program.manifest.as_ref().map(render::render_module_manifest))
}
