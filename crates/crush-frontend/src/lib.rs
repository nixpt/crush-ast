pub mod ai_runtime;
pub mod cson_desugar;
pub mod cast_enrich;
pub mod compiler;
mod cosm_kind;
pub mod cosm_lower;
pub mod diagnostics;
pub mod exhaustive_check;
pub mod import_system;
pub mod lang_imports;
pub mod language_walkers;
pub mod optimizer;
pub mod parser;
pub mod render;
pub mod semantics;
pub mod types;
pub mod mutation_check;
pub mod wip_check;
mod walker_intrinsics;

use anyhow::Result;

/// A semantic (type-check) error, with its source position when the checker
/// knows one. `Display` is the bare message — identical to the plain error it
/// replaces — so text consumers are unaffected; CLIs recover the class and the
/// position with `err.downcast_ref::<SemanticError>()` (CRUSH-315).
#[derive(Debug, Clone)]
pub struct SemanticError {
    /// 1-based source line and column of the offending construct, if known.
    pub pos: Option<(usize, usize)>,
    /// The error text, without any position prefix.
    pub message: String,
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SemanticError {}

/// Mark `err` as a semantic error (position-less if it has none yet). Errors
/// the checker already located pass through unchanged.
pub fn semantic_error(err: anyhow::Error) -> anyhow::Error {
    if err.downcast_ref::<SemanticError>().is_some() {
        return err;
    }
    anyhow::Error::new(SemanticError {
        pos: None,
        message: err.to_string(),
    })
}

/// Every syntax error of a source that failed to parse, kept structured so a
/// CLI can report each with its own position and code. `Display` is the text
/// `parse_source` has always produced (`Parse errors: a, b`) (CRUSH-315).
#[derive(Debug, Clone)]
pub struct ParseErrors(pub Vec<parser::ParseError>);

impl std::fmt::Display for ParseErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse errors: ")?;
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseErrors {}

/// Parse Crush source code into an enriched CAST Program.
///
/// Runs the parser then the CAST enrichment pass which populates
/// `Program.exhaustive_sites` from match expressions found in function bodies.
/// Annotations from `@module`, `@invariant`, `@errors` etc. are already in
/// the CAST from the parser; enrichment adds derived fields that require
/// walking the full program tree.
///
/// For warnings (e.g. exhaustiveness), use [`check_source`] instead.
pub fn parse_source(source: &str) -> Result<crush_cast::Program> {
    let mut program = parser::Parser::parse(source).map_err(ParseErrors)?;
    cast_enrich::enrich_cast(&mut program);
    Ok(program)
}

/// Parse Crush source, enrich the CAST, and run all compiler analysis passes.
///
/// Returns the enriched program alongside any diagnostics (warnings/errors).
/// Unlike `parse_source`, this does **not** short-circuit on analysis findings —
/// a `Vec` of `CompilerDiagnostic` is returned even when non-empty.
///
/// Parse failures (syntax errors) are still returned as `Err`.
pub fn check_source(
    source: &str,
) -> Result<(crush_cast::Program, Vec<diagnostics::CompilerDiagnostic>)> {
    let program = parse_source(source)?;
    let mut diags = exhaustive_check::check_exhaustiveness(&program);
    diags.extend(wip_check::check_wip(&program));
    diags.extend(wip_check::check_temporaries(&program));
    diags.extend(mutation_check::check_mutation_ordering(&program));
    Ok((program, diags))
}

/// Compile an already-parsed CAST Program into CASM bytecode.
///
/// Runs semantic analysis, optimization, and code generation. Clones the
/// program so the caller keeps the original; callers that are done with
/// their `Program` should use [`compile_cast_owned`] to skip the deep clone.
pub fn compile_cast(program: &crush_cast::Program) -> Result<casm::Program> {
    compile_cast_owned(program.clone())
}

/// Compile an already-parsed CAST Program into CASM bytecode, consuming it.
///
/// Identical to [`compile_cast`] but takes the program by value, avoiding a
/// deep clone of the whole AST on the hot compile path.
pub fn compile_cast_owned(program: crush_cast::Program) -> Result<casm::Program> {
    Ok(compile_cast_with_cosm(program)?.0)
}

/// Compile a CAST Program and also return its COSM module (COSM K1.2,
/// CRUSH-zorro-41): the `KindDecl`s / `InterfaceDecl`s its `struct` /
/// `impl` / `trait` / `@kind(…)` declarations lower to, validated — merged
/// with any module the walker supplied in `Program::cosm` (CRUSH-277 S1;
/// a name both sides declare is an error). `None` for a program that
/// declares none of them.
///
/// The CASM program carries the same module as its kind table
/// (`casm::Program::cosm`, COSM K1.3, CRUSH-zorro-42); its functions include
/// the lifted methods (`Point::norm2`, …).
pub fn compile_cast_with_cosm(
    program: crush_cast::Program,
) -> Result<(casm::Program, Option<crush_cosm::CosmModule>)> {
    let (mut program, cosm) = lower_and_check(program)?;
    optimizer::Optimizer::optimize(&mut program);
    let mut comp = compiler::Compiler::new();
    let casm = comp.compile_lowered(program, cosm.clone())?;
    Ok((casm, cosm))
}

/// The front half every compile path shares: lower the program's `struct` /
/// `impl` / `trait` / `@kind(…)` declarations (merging a walker-supplied COSM
/// module), then semantic-check it with the constructors of constructed Kinds
/// declared, so `Acc(10)` of a `@kind(construct: ctor(…))` Kind checks
/// (#260: `crushc` used to check without them). Returns the lowered program
/// and its module, ready for `Compiler::compile_lowered`.
pub fn lower_and_check(
    mut program: crush_cast::Program,
) -> Result<(crush_cast::Program, Option<crush_cosm::CosmModule>)> {
    let walker_cosm = program.cosm.take();
    let derived_cosm = cosm_lower::lower(&mut program).map_err(semantic_error)?;
    let cosm =
        cosm_lower::merge_walker_module(walker_cosm, derived_cosm).map_err(semantic_error)?;
    let mut sema = semantics::SemanticAnalyzer::new();
    if let Some(module) = &cosm {
        sema.declare_constructors(module, &program);
    }
    sema.check(&program).map_err(semantic_error)?;
    Ok((program, cosm))
}

/// Parse Crush source and compile directly to CASM bytecode.
pub fn compile_crush_source(source: &str) -> Result<casm::Program> {
    let program = parse_source(source)?;
    compile_cast_owned(program)
}

/// Parse Crush source and lower its `struct` / `impl` / `trait` / `@kind(…)`
/// declarations to a validated COSM module, without generating code (COSM
/// K1.2, CRUSH-zorro-41). Unlike compiling, this accepts every strategy in
/// the catalogue — including ones the VMs can't run yet. `None` when the
/// source declares none of them.
pub fn lower_to_cosm(source: &str) -> Result<Option<crush_cosm::CosmModule>> {
    let mut program = parse_source(source)?;
    cosm_lower::lower(&mut program)
}

/// Parse Crush source and compile it, also returning its COSM module (see
/// [`compile_cast_with_cosm`]).
pub fn compile_crush_source_with_cosm(
    source: &str,
) -> Result<(casm::Program, Option<crush_cosm::CosmModule>)> {
    compile_cast_with_cosm(parse_source(source)?)
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
