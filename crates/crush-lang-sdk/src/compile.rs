use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(any(
    feature = "polyglot-python",
    feature = "polyglot-javascript",
    feature = "polyglot-rust"
))]
use crush_walker_core_no_tree_sitter::AdapterRegistry;

static LABEL_COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_label() -> String {
    let n = LABEL_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("L{}", n)
}

fn get_slot(name: &str, map: &mut HashMap<String, u16>, next: &mut u16) -> u16 {
    if let Some(&slot) = map.get(name) {
        slot
    } else {
        let slot = *next;
        map.insert(name.to_string(), slot);
        *next += 1;
        slot
    }
}

fn cap_returns_value(name: &str) -> bool {
    if let Some(spec) = crush_vm::capabilities().get(name) {
        return spec.returns;
    }
    true
}

/// Compile Crush source for a debugger: the program (its `source_map` in
/// Crush source lines) and the names of its locals.
pub fn compile_crush_source_for_debug(
    source: &str,
) -> anyhow::Result<(crush_vm::Program, LocalNames)> {
    let (casm, warnings) = compile_crush_to_casm_with_options(source, &CompileOptions::default())?;
    let (mut program, names) = casm_to_vm_named(&casm)?;
    program.warnings = warnings;
    Ok((program, names))
}

pub fn compile_crush_source(source: &str) -> anyhow::Result<crush_vm::Program> {
    compile_crush_source_with(source, &CompileOptions::default())
}

/// [`compile_crush_source`] with [`CompileOptions`] — e.g.
/// `PolyglotMode::Lower` to lower `@python` / `@javascript` blocks into the
/// program instead of running them in an interpreter.
pub fn compile_crush_source_with(
    source: &str,
    options: &CompileOptions,
) -> anyhow::Result<crush_vm::Program> {
    let (casm, warnings) = compile_crush_to_casm_with_options(source, options)?;
    let mut program = casm_to_vm(&casm)?;
    program.warnings = warnings;
    Ok(program)
}

/// Compile Crush source or an enabled walker language into a CVM1 program.
///
/// `language_or_extension` accepts a registered language name (`"rust"`) or
/// extension (`"rs"`, `".rs"`). Rust source support requires the
/// `polyglot-rust` feature. Walker frontends share `AdapterRegistry` and the
/// same CAST → CASM → CVM1 pipeline. Crush source keeps its existing polyglot
/// preparation and declared-capability validation.
pub fn compile_source(
    language_or_extension: &str,
    source: &str,
) -> anyhow::Result<crush_vm::Program> {
    let selector = language_or_extension.trim();
    let normalized = selector.trim_start_matches('.').to_ascii_lowercase();
    if normalized == "crush" || normalized == "cr" {
        return compile_crush_source(source);
    }

    #[cfg(any(
        feature = "polyglot-python",
        feature = "polyglot-javascript",
        feature = "polyglot-rust"
    ))]
    {
        let registry = source_adapter_registry();
        let (_report, cast) = registry.walk_language_or_extension(source, selector)?;
        let casm = crush_frontend::compile_cast_owned(cast)?;
        return casm_to_vm(&casm);
    }
    #[cfg(not(any(
        feature = "polyglot-python",
        feature = "polyglot-javascript",
        feature = "polyglot-rust"
    )))]
    anyhow::bail!("no walker adapters enabled for source `{selector}`")
}

/// Rust convenience wrapper over [`compile_source`].
#[cfg(feature = "polyglot-rust")]
pub fn compile_rust_source(source: &str) -> anyhow::Result<crush_vm::Program> {
    compile_source("rust", source)
}

#[cfg(any(
    feature = "polyglot-python",
    feature = "polyglot-javascript",
    feature = "polyglot-rust"
))]
fn source_adapter_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    #[cfg(feature = "polyglot-python")]
    registry.register(Box::new(crush_lang_python::PythonAdapter));
    #[cfg(feature = "polyglot-javascript")]
    registry.register(Box::new(crush_lang_js::JsAdapter));
    #[cfg(feature = "polyglot-rust")]
    registry.register(Box::new(crush_lang_rust::RustAdapter));
    registry
}

/// Crush source → CASM, including [`prepare_polyglot_blocks`]. Use this,
/// not `crush_frontend::compile_crush_source`, from any tool that compiles
/// Crush source: the frontend can't run the polyglot pass itself (it needs
/// the Python analyzer), so the bare frontend entry point leaves `@lang`
/// blocks unmarshaled (CRUSH-130).
pub fn compile_crush_to_casm(source: &str) -> anyhow::Result<casm::Program> {
    Ok(compile_crush_to_casm_with_warnings(source)?.0)
}

/// [`compile_crush_to_casm`] plus the compile warnings collected by the
/// polyglot pass (CRUSH-270's implicit-output and clobber warnings). Callers
/// that surface warnings to the author (crush-run → stderr) use this and
/// carry the strings alongside the program.
pub fn compile_crush_to_casm_with_warnings(
    source: &str,
) -> anyhow::Result<(casm::Program, Vec<String>)> {
    compile_crush_to_casm_with_options(source, &CompileOptions::default())
}

/// [`compile_crush_to_casm_with_warnings`] with [`CompileOptions`].
pub fn compile_crush_to_casm_with_options(
    source: &str,
    options: &CompileOptions,
) -> anyhow::Result<(casm::Program, Vec<String>)> {
    compile_program_to_casm(crush_frontend::parse_source(source)?, options)
}

/// Compile the Crush file at `path`, resolving its imports (CRUSH-110) from
/// its directory and then `imports`' search path; the CASM plus the polyglot
/// pass's warnings. What `crushc` / `crush-run` compile a `.crush` file with.
/// The string entry points ([`compile_crush_to_casm_with_options`], …) resolve
/// no imports: a program with `import a.b` compiled through them is refused
/// with an error saying it needs a file path.
pub fn compile_crush_file_to_casm(
    path: &std::path::Path,
    imports: &crush_frontend::import_system::ImportOptions,
    options: &CompileOptions,
) -> anyhow::Result<(casm::Program, Vec<String>)> {
    compile_program_to_casm(crush_frontend::parse_file(path, imports)?, options)
}

/// [`compile_crush_file_to_casm`] to a CVM1 program (warnings attached).
pub fn compile_crush_file(
    path: &std::path::Path,
    imports: &crush_frontend::import_system::ImportOptions,
    options: &CompileOptions,
) -> anyhow::Result<crush_vm::Program> {
    let (casm, warnings) = compile_crush_file_to_casm(path, imports, options)?;
    let mut program = casm_to_vm(&casm)?;
    program.warnings = warnings;
    Ok(program)
}

/// [`compile_crush_source_for_debug`] for a file, resolving its imports.
pub fn compile_crush_file_for_debug(
    path: &std::path::Path,
    imports: &crush_frontend::import_system::ImportOptions,
) -> anyhow::Result<(crush_vm::Program, LocalNames)> {
    let (casm, warnings) = compile_crush_file_to_casm(path, imports, &CompileOptions::default())?;
    let (mut program, names) = casm_to_vm_named(&casm)?;
    program.warnings = warnings;
    Ok((program, names))
}

/// The shared back half of the source and file entry points.
fn compile_program_to_casm(
    mut program: crush_cast::Program,
    options: &CompileOptions,
) -> anyhow::Result<(casm::Program, Vec<String>)> {
    let declared = program.manifest.as_ref().and_then(|m| m.capabilities.clone());
    let (casm, warnings) = match options.polyglot {
        PolyglotMode::Exec => {
            let warnings = prepare_polyglot_blocks(&mut program)?;
            (crush_frontend::compile_cast_owned(program)?, warnings)
        }
        PolyglotMode::Lower => compile_lowering_blocks(program)?,
    };
    if let Some(declared) = declared {
        check_declared_capabilities(&casm, &declared)?;
    }
    Ok((casm, warnings))
}

/// [`PolyglotMode::Lower`]'s pipeline. `compile_cast_owned` checks the whole
/// program with Crush's semantic checker and optimizes it; walker CAST is
/// neither written for that checker (the walkers' own pipeline compiles
/// unchecked) nor for the optimizer. So the Crush program is checked as
/// `Exec` would see it — each block opaque, its outputs untyped — then its
/// blocks are lowered, the Crush functions optimized, and the lowered block
/// functions added, as the walkers' pipeline compiles them.
fn compile_lowering_blocks(
    mut program: crush_cast::Program,
) -> anyhow::Result<(casm::Program, Vec<String>)> {
    let cosm = crush_frontend::cosm_lower::lower(&mut program)?;
    {
        let mut opaque = program.clone();
        prepare_polyglot_blocks(&mut opaque)?;
        let mut sema = crush_frontend::semantics::SemanticAnalyzer::new();
        if let Some(module) = &cosm {
            sema.declare_constructors(module, &opaque);
        }
        sema.check(&opaque)?;
    }
    let (warnings, lowered) = prepare_blocks(&mut program, PolyglotMode::Lower)?;
    crush_frontend::optimizer::Optimizer::optimize(&mut program);
    let blocks: Vec<crush_cast::Function> = lowered.iter().map(|(_, f)| f.clone()).collect();
    program.functions.extend(lowered);
    let casm = crush_frontend::compiler::Compiler::new().compile_lowered(program, cosm)?;
    if let Some((name, line, lang)) = unknown_builtin(&casm, &blocks)? {
        anyhow::bail!(
            "line {line}: this @{lang} block can't be lowered inline: {} is not supported by the \
             walker yet (it has no Crush lowering)\n  {}",
            describe_unlowered(&name),
            lowering_help(&lang)
        );
    }
    Ok((casm, warnings))
}

/// Compile a whole Python or JavaScript script the way [`PolyglotMode::Lower`]
/// compiles a block: through the walker, exact or loud. Stricter than
/// [`compile_source`], which compiles whatever the walker produces: here
/// Python tuples and module attributes, builtins with no Crush lowering
/// (`sum`, `int`, …) and other `import`s are compile errors rather than lists,
/// unset locals or "unknown capability" failures when reached. JS goes through
/// the boa lowering. The script runs **as Crush**, not as CPython / Node.
///
/// `language_or_extension`: `python` / `py` / `javascript` / `js` (a leading
/// `.` is fine). Needs the language's `polyglot-*` feature.
pub fn compile_lowered_script(
    language_or_extension: &str,
    source: &str,
) -> anyhow::Result<crush_vm::Program> {
    let selector = language_or_extension.trim().trim_start_matches('.').to_ascii_lowercase();
    let lang = match selector.as_str() {
        "py" | "pyw" => "python",
        "mjs" | "cjs" => "javascript",
        other => match crush_frontend::lang_imports::canonical_lang(other) {
            Some(lang @ ("python" | "javascript")) => lang,
            _ => anyhow::bail!("no lowered-script support for `{language_or_extension}` (python or javascript)"),
        },
    };
    let (guest, interpreter) = if lang == "python" { ("Python", "CPython") } else { ("JavaScript", "Node") };
    let refuse = |why: String| {
        anyhow::anyhow!(
            "this {guest} script can't be lowered: {why}\n  it runs as Crush (its {guest} lowered \
             through the walker), not as {interpreter}; a script that needs this has to run in a \
             real interpreter"
        )
    };
    let (mut main, helpers) = lower_block_function(lang, source, &[], &[], 1, "__crush_script__")
        .map_err(|e| refuse(format!("{e:#}")))?;
    main.meta.insert("polyglot_block_line".to_string(), serde_json::json!(1));
    main.meta.insert("polyglot_block_lang".to_string(), serde_json::json!(lang));
    let mut blocks = vec![main.clone()];
    blocks.extend(helpers.iter().map(|(_, f)| f.clone()));
    let mut functions: HashMap<String, crush_cast::Function> = helpers.into_iter().collect();
    functions.insert("main".to_string(), main);
    let program = crush_cast::Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some(lang.to_string()),
        functions,
        ..Default::default()
    };
    let casm = crush_frontend::compiler::Compiler::new().compile(program)?;
    if let Some((name, _, _)) = unknown_builtin(&casm, &blocks)? {
        return Err(refuse(format!(
            "{} is not supported by the walker yet (it has no Crush lowering)",
            describe_unlowered(&name)
        )));
    }
    casm_to_vm(&casm)
}

/// What [`unknown_builtin`] found, as the source construct: a builtin by name
/// (`` `sum` ``), but a walker intrinsic by what the author wrote, not by the
/// internal name the walker lowered it to (#312: `x in y` was reported as
/// `` `__crush_contains__` ``).
fn describe_unlowered(name: &str) -> String {
    match name {
        "__crush_contains__" => "the `in` operator (`x in y`)".to_string(),
        n if n.starts_with("__crush_") && n.ends_with("__") => {
            format!("a construct the walker lowers to its internal `{n}`")
        }
        n => format!("`{n}`"),
    }
}

/// A Python / JS builtin the walker has no Crush lowering for (`sum`, `int`,
/// `abs`, …) compiles to a call of a capability of that bare name, which no
/// host provides: it would fail when reached ("unknown capability: sum").
/// Crush source can't produce such a call (its checker refuses an undefined
/// function), so any is a lowered block's — refuse it now, naming the block.
/// The first such builtin, with the line and language of the block using it.
#[allow(clippy::type_complexity)]
fn unknown_builtin(
    casm: &casm::Program,
    blocks: &[crush_cast::Function],
) -> anyhow::Result<Option<(String, u64, String)>> {
    if blocks.is_empty() {
        return Ok(None);
    }
    let used = crate::effects::used_by(&casm_to_vm(casm)?)?;
    let builtins = crush_vm::capabilities();
    let catalog = crate::effects::catalog();
    let Some(name) = used.iter().find(|cap| {
        !cap.contains('.')
            && !builtins.contains_key(cap.as_str())
            && !catalog.iter().any(|c| &c.name == *cap)
    }) else {
        return Ok(None);
    };
    let needle = serde_json::json!(name);
    let mentions = |v: &serde_json::Value| -> bool {
        // `math.trunc(x)` compiles to a capability named `trunc`.
        fn names(x: &serde_json::Value, needle: &serde_json::Value) -> bool {
            x == needle
                || matches!((x.as_str(), needle.as_str()), (Some(x), Some(n)) if x.ends_with(&format!(".{n}")))
        }
        fn walk(v: &serde_json::Value, needle: &serde_json::Value) -> bool {
            match v {
                serde_json::Value::Object(m) => m
                    .iter()
                    .any(|(k, x)| ((k == "function" || k == "name") && names(x, needle)) || walk(x, needle)),
                serde_json::Value::Array(xs) => xs.iter().any(|x| walk(x, needle)),
                _ => false,
            }
        }
        walk(v, &needle)
    };
    let block = blocks.iter().find(|f| serde_json::to_value(&f.body).is_ok_and(|b| mentions(&b)));
    let (line, lang) = block
        .map(|f| {
            (
                f.meta.get("polyglot_block_line").and_then(|l| l.as_u64()).unwrap_or(0),
                f.meta.get("polyglot_block_lang").and_then(|l| l.as_str()).unwrap_or("python").to_string(),
            )
        })
        .unwrap_or((0, "python".to_string()));
    Ok(Some((name.clone(), line, lang)))
}

/// CRUSH-243: a program that declares `@capabilities` may only use the
/// ambient capabilities and the ones its entries cover. Checked on the
/// bytecode, so the names are exactly the ones the VM will gate on
/// (builtins lowered to capability calls, `polyglot.<lang>`, AI gates).
fn check_declared_capabilities(casm: &casm::Program, declared: &[String]) -> anyhow::Result<()> {
    let used = crate::effects::used_by(&casm_to_vm(casm)?)?;
    let undeclared = crate::effects::undeclared(&used, declared);
    if undeclared.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "the program uses {} not declared in @capabilities: {}\n  \
         add {} to @capabilities, or stop using {}",
        if undeclared.len() == 1 { "a capability" } else { "capabilities" },
        undeclared.join(", "),
        if undeclared.len() == 1 { "it" } else { "them" },
        if undeclared.len() == 1 { "it" } else { "them" },
    )
}

/// Every path from Crush source to bytecode must run this — `crushc` used to
/// skip it, so `crushc x.crush -o x.cvm1` rejected or mis-ran any program
/// whose `@lang` block produces a variable (CRUSH-130).
///
/// Fill in `Statement::LangBlock.variables` (inputs) and
/// `meta["polyglot_output"]` (the single output var, per the current
/// exec_lang protocol) for every `@python { ... }` block, via real
/// free-variable analysis over the block's own AST — never a regex, never
/// a blind "inject everything in scope". See
/// `crush_lang_python::analyzer::free_variables`.
///
/// Other languages (no parser wired yet) are left alone: their blocks
/// compile with no marshaling, the same behavior as before this pass
/// existed — not a regression, just not-yet-implemented. A malformed
/// Python block is left unmarshaled too rather than failing Crush
/// compilation outright; the actual `python3` subprocess will raise its
/// own loud syntax error at run time, which is still honest, just later.
///
/// It also fills `LangBlock.imports` with the `use @lang` imports that reach
/// each block (CRUSH-224); the compiler writes them into the block's source.
/// A `use @lang` reaches every later block of its language in the same body
/// and the bodies nested in it. One at the top level of `main` — where a
/// script's top-level statements land — also reaches every other function.
///
/// Returns the compile warnings collected while walking the blocks — the
/// implicit-output and clobber warnings of the CRUSH-270 transition — for
/// the caller to surface (crush-run prints them to stderr). Errors (a
/// declared `->` output the block never binds, `->` on a language with no
/// marshaling) are returned as `Err`.
pub fn prepare_polyglot_blocks(
    program: &mut crush_cast::Program,
) -> anyhow::Result<Vec<String>> {
    prepare_polyglot_blocks_with(program, PolyglotMode::Exec)
}

/// [`prepare_polyglot_blocks`] with a choice of what a `@python` /
/// `@javascript` block becomes; see [`PolyglotMode`].
pub fn prepare_polyglot_blocks_with(
    program: &mut crush_cast::Program,
    mode: PolyglotMode,
) -> anyhow::Result<Vec<String>> {
    let (warnings, lowered) = prepare_blocks(program, mode)?;
    program.functions.extend(lowered);
    Ok(warnings)
}

/// The pass itself; the functions made from lowered blocks are returned, not
/// yet added to the program.
#[allow(clippy::type_complexity)]
fn prepare_blocks(
    program: &mut crush_cast::Program,
    mode: PolyglotMode,
) -> anyhow::Result<(Vec<String>, Vec<(String, crush_cast::Function)>)> {
    let file_level: Vec<crush_cast::ImportStatement> = program
        .functions
        .get("main")
        .map(|main| main.body.iter().filter_map(polyglot_use).cloned().collect())
        .unwrap_or_default();
    let mut warnings: Vec<String> = Vec::new();
    let mut pass = Pass {
        mode,
        warnings: &mut warnings,
        crush_functions: program.functions.keys().cloned().collect(),
        lowered: Vec::new(),
    };
    for (name, func) in program.functions.iter_mut() {
        let mut known_locals: HashSet<String> =
            func.params.iter().map(|(name, _)| name.clone()).collect();
        let mut uses = if name == "main" { Vec::new() } else { file_level.clone() };
        prepare_stmts(&mut func.body, &mut known_locals, &mut uses, &mut pass)?;
    }
    let lowered = std::mem::take(&mut pass.lowered);
    if mode == PolyglotMode::Lower {
        refuse_unlowered_blocks(program)?;
    }
    Ok((warnings, lowered))
}

/// What [`prepare_polyglot_blocks_with`] makes of a `@python` / `@javascript`
/// block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PolyglotMode {
    /// `EXEC_LANG`: the block runs in a real interpreter (`python3`, `node`,
    /// or an embedder's executor), behind the `polyglot.<lang>` grant. Inputs
    /// and outputs cross as JSON. The default, and what `crush-run` does.
    #[default]
    Exec,
    /// Inline lowering (CRUSH-272): the block's body is lowered through the
    /// language's walker into the enclosing program's CAST, as one function
    /// called where the block stands. Its inputs (the Crush variables it reads)
    /// are copied in and its outputs copied back, as JSON would (the
    /// `lang.marshal` capability), so it can't change a Crush variable it
    /// doesn't declare. It spawns nothing, so it needs no `polyglot.<lang>`
    /// grant.
    ///
    /// The block then runs **as Crush**: Python / JavaScript lowered by the
    /// walker, not CPython / Node. Exact or loud: a construct the walker can't
    /// lower fails compilation, naming the construct and the block's line,
    /// never falling back to `EXEC_LANG` or changing meaning. Needs the
    /// language's `polyglot-python` / `polyglot-javascript` feature. Blocks in
    /// other languages (`@bash`, …) are unaffected.
    Lower,
}

/// Compile options for Crush source.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompileOptions {
    /// What `@python` / `@javascript` blocks become.
    pub polyglot: PolyglotMode,
}

/// State shared across one [`prepare_polyglot_blocks_with`] run.
struct Pass<'a> {
    mode: PolyglotMode,
    warnings: &'a mut Vec<String>,
    /// The program's own functions: a lowered block must not call them by
    /// accident (a real interpreter can't see them).
    crush_functions: HashSet<String>,
    /// Functions made from lowered blocks, added to the program at the end.
    lowered: Vec<(String, crush_cast::Function)>,
}

/// For a build without the block language's analyzer (its `polyglot-*`
/// feature, e.g. crush-web without `polyglot-python`): the block's declared
/// `->` outputs still become Crush variables, so the program compiles and the
/// capability check refuses an ungranted block exactly as a native run does,
/// instead of compilation failing on an "undefined" output. The block itself
/// stays unmarshaled; run anyway, it fails loudly ("produced no marshaled
/// outputs").
#[allow(dead_code)]
fn declare_unanalyzed_outputs(
    outputs: &[String],
    meta: &mut HashMap<String, serde_json::Value>,
    known_locals: &mut HashSet<String>,
) {
    if outputs.is_empty() {
        return;
    }
    meta.insert("polyglot_outputs".to_string(), serde_json::json!(outputs));
    known_locals.extend(outputs.iter().cloned());
}

/// The import of a `use @lang …` statement.
fn polyglot_use(stmt: &crush_cast::Statement) -> Option<&crush_cast::ImportStatement> {
    match stmt {
        crush_cast::Statement::Import {
            import: import @ crush_cast::ImportStatement::PolyglotModule { .. },
            ..
        } => Some(import),
        _ => None,
    }
}

fn prepare_stmts(
    stmts: &mut Vec<crush_cast::Statement>,
    known_locals: &mut HashSet<String>,
    uses: &mut Vec<crush_cast::ImportStatement>,
    pass: &mut Pass<'_>,
) -> anyhow::Result<()> {
    use crush_cast::Statement;
    let mut i = 0;
    while i < stmts.len() {
        let stmt = &mut stmts[i];
        i += 1;
        if let Some(import) = polyglot_use(stmt) {
            uses.push(import.clone());
            continue;
        }
        if let Statement::LangBlock { lang, imports, .. } = stmt {
            let canonical = crush_frontend::lang_imports::canonical_lang(lang);
            *imports = uses
                .iter()
                .filter(|u| {
                    matches!(u, crush_cast::ImportStatement::PolyglotModule { language, .. }
                        if crush_frontend::lang_imports::canonical_lang(language) == canonical
                            && canonical.is_some())
                })
                .cloned()
                .collect();
        }
        if pass.mode == PolyglotMode::Lower
            && let Statement::LangBlock { lang, .. } = stmt
            && is_lowerable_lang(lang)
        {
            let replacement = lower_block(stmt, known_locals, pass)?;
            let n = replacement.len();
            stmts.splice(i - 1..i, replacement);
            i += n - 1;
            continue;
        }
        #[allow(unused_variables)] // used only with a polyglot-* analyzer
        let warnings = &mut *pass.warnings;
        match stmt {
            Statement::VarDecl { name, .. }
            | Statement::Assign { target: name, .. }
            | Statement::Export { name, .. } => {
                known_locals.insert(name.clone());
            }
            Statement::LangBlock {
                lang,
                code,
                variables,
                outputs,
                meta,
                ..
            } if lang == "python"
                || lang == "javascript"
                || lang == "js"
                || lang == "node" =>
            {
                // The lexer captures the `{ ... }` body verbatim, including
                // whatever leading indentation it happened to sit at inside
                // `@python { ... }`. Guest `-c`/`-e` tolerates that on its own
                // (indentation is only ever relative for Python; JS is
                // brace-delimited), but analyzers here do not — dedent first
                // so analysis sees the same shape the subprocess would
                // execute. The real `code` sent to the subprocess is
                // dedented too, but only if a prologue/epilogue actually
                // gets spliced in below; an unmarshaled block is left
                // completely untouched.
                if lang == "python" {
                    #[cfg(feature = "polyglot-python")]
                    if let Ok(free_vars) =
                        crush_lang_python::analyzer::free_variables(&dedent(code))
                    {
                        let crush_lang_python::analyzer::FreeVars {
                            reads,
                            top_level_bound,
                            bound_anywhere,
                        } = free_vars;
                        let inputs: Vec<String> = reads
                            .into_iter()
                            .filter(|name| known_locals.contains(name))
                            .collect();
                        let output_names = resolve_outputs(
                            lang,
                            outputs,
                            &top_level_bound,
                            &bound_anywhere,
                            known_locals,
                            warnings,
                        )?;
                        if !output_names.is_empty() {
                            meta.insert(
                                "polyglot_outputs".to_string(),
                                serde_json::json!(output_names),
                            );
                            for name in &output_names {
                                known_locals.insert(name.clone());
                            }
                        }
                        if !inputs.is_empty() || !output_names.is_empty() {
                            *code = rewrite_python_marshaling(code, &inputs, &output_names);
                        }
                        *variables = inputs;
                    }
                    #[cfg(not(feature = "polyglot-python"))]
                    let _ = (lang, code, variables);
                    #[cfg(not(feature = "polyglot-python"))]
                    declare_unanalyzed_outputs(outputs, meta, known_locals);
                } else {
                    // javascript / js / node
                    #[cfg(feature = "polyglot-javascript")]
                    if let Ok(free_vars) = crush_lang_js::freevars::free_variables(&dedent(code)) {
                        let crush_lang_js::freevars::FreeVars {
                            reads,
                            top_level_bound,
                            bound_anywhere,
                        } = free_vars;
                        let inputs: Vec<String> = reads
                            .into_iter()
                            .filter(|name| known_locals.contains(name))
                            .collect();
                        let output_names = resolve_outputs(
                            lang,
                            outputs,
                            &top_level_bound,
                            &bound_anywhere,
                            known_locals,
                            warnings,
                        )?;
                        if !output_names.is_empty() {
                            meta.insert(
                                "polyglot_outputs".to_string(),
                                serde_json::json!(output_names),
                            );
                            for name in &output_names {
                                known_locals.insert(name.clone());
                            }
                        }
                        if !inputs.is_empty() || !output_names.is_empty() {
                            *code = rewrite_javascript_marshaling(code, &inputs, &output_names);
                        }
                        *variables = inputs;
                    }
                    #[cfg(not(feature = "polyglot-javascript"))]
                    let _ = (lang, code, variables);
                    #[cfg(not(feature = "polyglot-javascript"))]
                    declare_unanalyzed_outputs(outputs, meta, known_locals);
                }
            }
            // `->` on a language with no value marshaling is a loud error,
            // never a silent no-op: there is nothing to marshal back.
            Statement::LangBlock { lang, outputs, .. } if !outputs.is_empty() => {
                anyhow::bail!(
                    "`->` output lists are not supported for @{lang}: it has no value marshaling"
                );
            }
            // #255: @bash blocks lost their only input path when #245 removed
            // the env-only fallback (bash has no free-variable analyzer like
            // python/js). Scan the block for `$name` / `${name}` references
            // and inject the matching Crush locals as env vars, restoring the
            // documented pre-#245 behavior. Anything not referenced stays out.
            Statement::LangBlock {
                lang, code, variables, ..
            } if lang == "bash" || lang == "sh" => {
                let refs = bash_var_refs(code);
                let inputs: Vec<String> = refs
                    .into_iter()
                    .filter(|name| known_locals.contains(name))
                    .collect();
                *variables = inputs;
            }
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                prepare_stmts(then_body, known_locals, &mut uses.clone(), pass)?;
                if let Some(else_body) = else_body {
                    prepare_stmts(else_body, known_locals, &mut uses.clone(), pass)?;
                }
            }
            Statement::While { body, .. } => {
                prepare_stmts(body, known_locals, &mut uses.clone(), pass)?
            }
            Statement::For {
                variable, body, ..
            } => {
                known_locals.insert(variable.clone());
                prepare_stmts(body, known_locals, &mut uses.clone(), pass)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Resolve a block's marshaled outputs from its declared `->` list (or, in
/// the one-release transition, its implicit last-top-level-assignment rule).
///
/// CRUSH-270: with `->`, every declared name must be bound somewhere in the
/// block (a hard compile error otherwise) and each becomes a Crush variable
/// at this line. Without `->`, the old implicit rule still applies, but the
/// caller records a warning naming the exported variable (and, when it
/// overwrites an existing Crush variable, the clobber).
fn resolve_outputs(
    lang: &str,
    declared: &[String],
    top_level_bound: &[String],
    bound_anywhere: &HashSet<String>,
    known_locals: &HashSet<String>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Vec<String>> {
    if !declared.is_empty() {
        for name in declared {
            if !bound_anywhere.contains(name) {
                anyhow::bail!(
                    "@{lang} block declares output `{name}` but never binds it; \
                     add `{name} = …` at the block's top level, or drop it from `->`"
                );
            }
        }
        return Ok(declared.to_vec());
    }

    match top_level_bound.last() {
        Some(name) => {
            warnings.push(format!(
                "@{lang} block exports `{name}` implicitly; add `-> {name}` to declare it"
            ));
            if known_locals.contains(name) {
                warnings.push(format!(
                    "@{lang} block overwrites the existing Crush variable `{name}`; \
                     declare `-> {name}` to make the overwrite explicit"
                ));
            }
            Ok(vec![name.clone()])
        }
        None => Ok(Vec::new()),
    }
}

/// Languages [`PolyglotMode::Lower`] lowers (it leaves the others to
/// `EXEC_LANG`).
fn is_lowerable_lang(lang: &str) -> bool {
    matches!(
        crush_frontend::lang_imports::canonical_lang(lang),
        Some("python") | Some("javascript")
    )
}

/// The help every inline-lowering refusal carries.
fn lowering_help(lang: &str) -> String {
    let (guest, interpreter) = match lang {
        "python" => ("Python", "CPython"),
        _ => ("JavaScript", "Node"),
    };
    format!(
        "inline lowering runs a block as Crush (its {guest} lowered through the walker), \
         not as {interpreter}; a block that needs this has to run in a real interpreter: \
         compile without inline lowering and grant polyglot.{lang} (crush-run --polyglot)"
    )
}

/// [`PolyglotMode::Lower`] for one `@python` / `@javascript` block: lower its
/// body into a new function (recorded in `pass.lowered`) and return the
/// statements that replace the block: the call, with each input copied in,
/// and each declared output copied back out into its Crush variable.
fn lower_block(
    stmt: &crush_cast::Statement,
    known_locals: &mut HashSet<String>,
    pass: &mut Pass<'_>,
) -> anyhow::Result<Vec<crush_cast::Statement>> {
    use crush_cast::{CastType, Expression, Statement};
    let Statement::LangBlock { lang, code, outputs, imports, deps, meta, .. } = stmt else {
        unreachable!("lower_block takes a LangBlock");
    };
    let lang = crush_frontend::lang_imports::canonical_lang(lang).unwrap_or("python");
    let line = meta.get("line").and_then(|v| v.as_u64()).unwrap_or(0);
    let refuse = |why: String| -> anyhow::Error {
        anyhow::anyhow!(
            "line {line}: this @{lang} block can't be lowered inline: {why}\n  {}",
            lowering_help(lang)
        )
    };
    if !deps.is_empty() {
        return Err(refuse(format!("it asks for packages ({})", deps.join(", "))));
    }
    // `use @lang` imports reach the block as they would its subprocess.
    let code = match crush_frontend::lang_imports::import_header(lang, imports)
        .map_err(|e| anyhow::anyhow!(e))?
    {
        Some(header) => crush_frontend::lang_imports::splice_header(lang, code, &header),
        None => code.clone(),
    };

    let (reads, top_level_bound, bound_anywhere) = block_free_variables(lang, &code)
        .map_err(|e| refuse(e.to_string()))?;
    let inputs: Vec<String> = reads.iter().filter(|n| known_locals.contains(*n)).cloned().collect();
    // A name the block reads that is a Crush function and no variable: a real
    // interpreter can't see it (a `NameError`), so it must not quietly become
    // a call into Crush here.
    if let Some(name) = reads
        .iter()
        .find(|n| !known_locals.contains(*n) && pass.crush_functions.contains(*n))
    {
        return Err(refuse(format!(
            "it uses `{name}`, which is a Crush function; a polyglot block can't see Crush functions"
        )));
    }
    let output_names =
        resolve_outputs(lang, outputs, &top_level_bound, &bound_anywhere, known_locals, pass.warnings)?;

    let n = pass.lowered.len();
    let mut fname = format!("__crush_{lang}_block_{n}");
    while pass.crush_functions.iter().any(|f| f.starts_with(&fname)) {
        fname.push('_');
    }
    let (mut function, helpers) =
        lower_block_function(lang, &code, &inputs, &output_names, line, &format!("{fname}__"))
            .map_err(|e| refuse(format!("{e:#}")))?;
    let mark = |f: &mut crush_cast::Function| {
        f.meta.insert("polyglot_block_line".to_string(), serde_json::json!(line));
        f.meta.insert("polyglot_block_lang".to_string(), serde_json::json!(lang));
    };
    mark(&mut function);
    pass.crush_functions.insert(fname.clone());
    pass.lowered.push((fname.clone(), function));
    for (name, mut helper) in helpers {
        mark(&mut helper);
        pass.crush_functions.insert(name.clone());
        pass.lowered.push((name, helper));
    }

    let str_lit = |s: &str| Expression::StringLiteral { value: s.to_string(), meta: meta.clone() };
    let marshal = |value: Expression, name: &str| Expression::CapabilityCall {
        name: "lang.marshal".to_string(),
        args: vec![value, str_lit(name)],
        meta: meta.clone(),
    };
    let call = Expression::Call {
        function: fname,
        args: inputs
            .iter()
            .map(|name| marshal(Expression::Var { name: name.clone(), meta: meta.clone() }, name))
            .collect(),
        meta: meta.clone(),
    };
    // The block prints what EXEC_LANG prints (#312): its output is held while
    // it runs and emitted trimmed when it returns; a throw out of it is fatal.
    let capture = |name: &str, args: Vec<Expression>| Statement::ExprStmt {
        expr: Expression::CapabilityCall { name: name.to_string(), args, meta: meta.clone() },
        meta: meta.clone(),
    };
    let begin = capture(
        "lang.capture_begin",
        vec![str_lit(lang), Expression::IntLiteral { value: line as i64, meta: meta.clone() }],
    );
    let end = capture("lang.capture_end", Vec::new());
    if output_names.is_empty() {
        return Ok(vec![begin, Statement::ExprStmt { expr: call, meta: meta.clone() }, end]);
    }
    let result = format!("__crush_{lang}_block_{n}_out");
    let mut out = vec![
        begin,
        Statement::VarDecl { name: result.clone(), value: call, type_hint: CastType::Any, meta: meta.clone() },
        end,
    ];
    for (k, name) in output_names.iter().enumerate() {
        let value = marshal(
            Expression::Index {
                target: Box::new(Expression::Var { name: result.clone(), meta: meta.clone() }),
                index: Box::new(Expression::IntLiteral { value: k as i64, meta: meta.clone() }),
                meta: meta.clone(),
            },
            name,
        );
        out.push(if known_locals.contains(name) {
            Statement::Assign { target: name.clone(), value, meta: meta.clone() }
        } else {
            Statement::VarDecl {
                name: name.clone(),
                value,
                type_hint: CastType::Any,
                meta: meta.clone(),
            }
        });
        known_locals.insert(name.clone());
    }
    Ok(out)
}

/// `(reads, top_level_bound, bound_anywhere)` of a block, from its language's
/// analyzer — or an error saying the feature isn't built.
#[allow(clippy::type_complexity)]
fn block_free_variables(
    lang: &str,
    code: &str,
) -> anyhow::Result<(Vec<String>, Vec<String>, HashSet<String>)> {
    match lang {
        #[cfg(feature = "polyglot-python")]
        "python" => {
            let fv = crush_lang_python::analyzer::free_variables(&dedent(code))
                .map_err(|e| anyhow::anyhow!("Python syntax error: {e}"))?;
            Ok((fv.reads, fv.top_level_bound, fv.bound_anywhere))
        }
        #[cfg(feature = "polyglot-javascript")]
        "javascript" => {
            let fv = crush_lang_js::freevars::free_variables(&dedent(code))
                .map_err(|e| anyhow::anyhow!("JavaScript syntax error: {e}"))?;
            Ok((fv.reads, fv.top_level_bound, fv.bound_anywhere))
        }
        _ => {
            let _ = code;
            anyhow::bail!("this build has no {lang} walker (crush-lang-sdk's `polyglot-{lang}` feature)")
        }
    }
}

/// The block's body as a function of `inputs` returning `[outputs...]`, and
/// the helper functions it needs (named `{prefix}…`).
#[allow(clippy::type_complexity)]
fn lower_block_function(
    lang: &str,
    code: &str,
    inputs: &[String],
    outputs: &[String],
    line: u64,
    prefix: &str,
) -> anyhow::Result<(crush_cast::Function, Vec<(String, crush_cast::Function)>)> {
    let _ = (code, inputs, outputs, line, prefix);
    match lang {
        #[cfg(feature = "polyglot-python")]
        "python" => crush_lang_python::inline::lower_block_function(&dedent(code), inputs, outputs)
            .map(|f| (f, Vec::new())),
        #[cfg(feature = "polyglot-javascript")]
        "javascript" => crush_lang_js::inline::lower_block_function(code, inputs, outputs, prefix)
            .map(|b| (b.function, b.helpers))
            .map_err(|e| {
            // The walker's line is relative to the block's body; name the
            // `.crush` line instead.
            match e.downcast::<crush_lang_js::lower::Unsupported>() {
                Ok(u) => match u.at {
                    Some((l, _)) => anyhow::anyhow!(
                        "{} is not supported by the JS walker yet (at line {})",
                        u.construct,
                        line + u64::from(l) - 1
                    ),
                    None => u.into(),
                },
                Err(e) => e,
            }
        }),
        _ => anyhow::bail!("this build has no {lang} walker (crush-lang-sdk's `polyglot-{lang}` feature)"),
    }
}

/// After [`PolyglotMode::Lower`], no `@python` / `@javascript` block may be
/// left to `EXEC_LANG`: one the pass didn't reach (inside a `try`, a lambda,
/// a `match` arm, …) is refused rather than quietly spawning an interpreter.
fn refuse_unlowered_blocks(program: &crush_cast::Program) -> anyhow::Result<()> {
    fn find(v: &serde_json::Value) -> Option<(String, u64)> {
        match v {
            serde_json::Value::Object(map) => {
                if map.get("type").and_then(|t| t.as_str()) == Some("LangBlock")
                    && let Some(lang) = map.get("lang").and_then(|l| l.as_str())
                    && is_lowerable_lang(lang)
                {
                    let line = map
                        .get("meta")
                        .and_then(|m| m.get("line"))
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0);
                    return Some((lang.to_string(), line));
                }
                map.values().find_map(find)
            }
            serde_json::Value::Array(items) => items.iter().find_map(find),
            _ => None,
        }
    }
    let json = serde_json::to_value(program)?;
    if let Some((lang, line)) = find(&json) {
        let lang = crush_frontend::lang_imports::canonical_lang(&lang).unwrap_or("python");
        anyhow::bail!(
            "line {line}: this @{lang} block can't be lowered inline: inline lowering doesn't \
             reach a block in this position yet (only function bodies and `if` / `while` / \
             `for` bodies)\n  {}",
            lowering_help(lang)
        );
    }
    Ok(())
}

/// Rewrite a `@python` block's source to bridge the Crush/Python value
/// boundary: `scheduler.rs`'s EXEC_LANG only gives the subprocess its
/// inputs as OS environment variables (always strings) and only gets a
/// value back by scanning stdout for `crush_vm::scheduler::CRUSH_RESULT_SENTINEL`
/// — the block's own bound names mean nothing to either side of that
/// boundary on their own.
///
/// - Inputs: JSON-decoded from `os.environ` into real Python locals
///   (round-trips int/float/bool/str/list/map, not just strings).
/// - Output: a JSON **object keyed by name** printed on a sentinel-prefixed
///   line, kept distinct from the block's own ordinary prints (which still
///   flow through as normal program output — stdout is a shared channel).
///   Each declared output becomes a key; the VM extracts them by name.
/// - A value that can't be JSON-encoded (NaN, a set, a custom object)
///   fails loudly, naming the output and its type, rather than being
///   silently dropped.
/// - A declared output the block only binds conditionally (so it is unbound
///   at run time) fails loudly via `NameError`, naming the variable — never
///   marshaled as `null`.
///
/// The whole input-decoding prologue is kept on a single physical line so
/// it shifts every line of the user's own code by exactly one line,
/// keeping Python traceback line numbers predictable rather than off by
/// a count that depends on how many variables happen to be marshaled.
#[cfg(feature = "polyglot-python")]
fn rewrite_python_marshaling(code: &str, inputs: &[String], outputs: &[String]) -> String {
    let mut prologue_parts: Vec<String> = Vec::new();
    if inputs.is_empty() {
        prologue_parts.push("import json as __crush_json".to_string());
    } else {
        prologue_parts.push("import json as __crush_json, os as __crush_os".to_string());
        for name in inputs {
            prologue_parts.push(format!(
                "{name} = __crush_json.loads(__crush_os.environ[{name:?}])"
            ));
        }
    }
    let prologue = prologue_parts.join("; ");

    // Deliberately hand-written as a literal Python string token — NOT
    // derived from `CRUSH_RESULT_SENTINEL` via `{:?}` — because Rust's
    // Debug escaping (`\0`) and Python's string-escape syntax (`\x00`)
    // are different grammars that happen to overlap for this one
    // character; relying on that coincidence would be fragile. This
    // MUST stay byte-for-byte in sync with
    // `crush_vm::scheduler::CRUSH_RESULT_SENTINEL` by hand.
    const PY_SENTINEL_LITERAL: &str = "\"\\x00CRUSH_RESULT\\x00\"";

    let epilogue = if outputs.is_empty() {
        String::new()
    } else {
        let mut lines = String::from("\ntry:\n    __crush_out = {}\n");
        for name in outputs {
            // `name` is an identifier (from the `->` list or a bound AST
            // name), never a quote — safe to splice as the value. The dict
            // key is JSON-quoted so any Unicode identifier round-trips.
            let key = serde_json::to_string(name).expect("output name serializes");
            lines.push_str(&format!("    __crush_out[{key}] = {name}\n"));
        }
        lines.push_str(&format!(
            "    print({PY_SENTINEL_LITERAL} + __crush_json.dumps(__crush_out))\n"
        ));
        lines.push_str("except NameError as __crush_missing:\n");
        lines.push_str("    import sys as __crush_sys\n");
        lines.push_str(
            "    print(\"output variable '\" + __crush_missing.name + \"' was not bound by the block\", file=__crush_sys.stderr)\n",
        );
        lines.push_str("    __crush_sys.exit(1)\n");
        lines.push_str("except TypeError as __crush_marshal_err:\n");
        lines.push_str("    import sys as __crush_sys\n");
        lines.push_str(&format!(
            "    print(\"cannot marshal output variable(s) {names} (a value was not JSON-serializable): \" + str(__crush_marshal_err), file=__crush_sys.stderr)\n",
            names = outputs.join(", "),
        ));
        lines.push_str("    __crush_sys.exit(1)\n");
        lines
    };

    // The block's own leading indentation (from however it sits inside
    // `@python { ... }`) is harmless on its own — `python3 -c` accepts a
    // uniformly-indented script, since indentation is only ever relative.
    // It becomes a real `IndentationError` the moment a column-0 prologue
    // line precedes it, so dedent first to keep the two consistent.
    format!("{prologue}\n{}{epilogue}", dedent(code))
}

/// Rewrite a `@javascript` block for the Crush/Node value boundary (CRUSH-68).
///
/// Same protocol as Python: JSON in env vars, a JSON object keyed by output
/// name printed on a sentinel-prefixed line out. Node reads `process.env`
/// (always strings) and `JSON.parse`s them.
#[cfg(feature = "polyglot-javascript")]
fn rewrite_javascript_marshaling(code: &str, inputs: &[String], outputs: &[String]) -> String {
    let mut prologue_parts: Vec<String> = Vec::new();
    for name in inputs {
        // `name` is an AST identifier — safe to splice as a JS binding.
        // Env key uses JSON.stringify so quotes/escapes match Node.
        prologue_parts.push(format!(
            "var {name} = JSON.parse(process.env[{}]);",
            serde_json::to_string(name).expect("string name")
        ));
    }
    let prologue = prologue_parts.join("");

    // Hand-synced with `crush_vm::scheduler::CRUSH_RESULT_SENTINEL`
    // (`\x00CRUSH_RESULT\x00`). JS string uses `\0` / `\x00` equivalently.
    const JS_SENTINEL_LITERAL: &str = "\"\\x00CRUSH_RESULT\\x00\"";

    let epilogue = if outputs.is_empty() {
        String::new()
    } else {
        let mut lines = String::from("\ntry {\n  var __crush_out = {};\n");
        for name in outputs {
            let key = serde_json::to_string(name).expect("output name serializes");
            lines.push_str(&format!("  __crush_out[{key}] = {name};\n"));
        }
        lines.push_str(&format!(
            "  console.log({JS_SENTINEL_LITERAL} + JSON.stringify(__crush_out));\n"
        ));
        lines.push_str("} catch (__crush_marshal_err) {\n");
        lines.push_str(&format!(
            "  console.error(\"cannot marshal output variable(s) {names}: \" + __crush_marshal_err);\n",
            names = outputs.join(", "),
        ));
        lines.push_str("  process.exit(1);\n}\n");
        lines
    };

    format!("{prologue}\n{}{epilogue}", dedent(code))
}

/// Strip the common leading whitespace shared by every non-blank line, so
/// a block's original relative indentation is preserved but its absolute
/// column no longer depends on how it happened to sit inside `{ ... }`.
#[cfg_attr(not(any(feature = "polyglot-python", feature = "polyglot-javascript")), allow(dead_code))]
fn dedent(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let common_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.get(common_indent..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Collect the variable names a `@bash` block references via `$name` or
/// `${name}` (#255). This is deliberately a dumb scanner, not a shell
/// parser: it skips single-quoted regions (no expansion there) and `$$`
/// (PID), and takes the longest valid identifier after `$`. False positives
/// (e.g. `$name` inside a double-quoted string that bash would expand
/// anyway) are harmless — an extra env var costs nothing. False negatives
/// would silently drop a variable, so when in doubt we include.
fn bash_var_refs(code: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bytes = code.as_bytes();
    let mut i = 0;
    let mut in_single_quote = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_single_quote {
            if b == b'\'' {
                in_single_quote = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' => {
                in_single_quote = true;
                i += 1;
            }
            b'$' => {
                let next = bytes.get(i + 1).copied();
                match next {
                    Some(b'{') => {
                        // ${name} — also ${name:-default} etc.; take up to
                        // the first non-identifier char.
                        let mut j = i + 2;
                        while j < bytes.len()
                            && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                        {
                            j += 1;
                        }
                        if j > i + 2 {
                            let name = &code[i + 2..j];
                            // Skip positional/special params like $1, $#, $@
                            if name
                                .chars()
                                .next()
                                .map(|c| c.is_alphabetic() || c == '_')
                                .unwrap_or(false)
                                && seen.insert(name.to_string())
                            {
                                refs.push(name.to_string());
                            }
                        }
                        i = j;
                    }
                    Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
                        // $name — longest identifier run
                        let mut j = i + 1;
                        while j < bytes.len()
                            && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                        {
                            j += 1;
                        }
                        let name = &code[i + 1..j];
                        if seen.insert(name.to_string()) {
                            refs.push(name.to_string());
                        }
                        i = j;
                    }
                    // $$ (PID), $1 (positional), $# etc.: not Crush vars
                    _ => {
                        i += 1;
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    refs
}

/// Local variable names per function, `function -> [(name, slot)]`: what
/// compiled bytecode forgets and a debugger needs to say `print total`.
pub type LocalNames = HashMap<String, Vec<(String, u16)>>;

pub fn casm_to_vm(program: &casm::Program) -> anyhow::Result<crush_vm::Program> {
    Ok(casm_to_vm_named(program)?.0)
}

/// [`casm_to_vm`] plus the [`LocalNames`] it assigned slots from. The program's
/// `source_map` maps bytecode offsets to the lines of the Crush source the CASM
/// came from (instructions that carry no line are left out).
pub fn casm_to_vm_named(
    program: &casm::Program,
) -> anyhow::Result<(crush_vm::Program, LocalNames)> {
    let mut local_names: LocalNames = HashMap::new();
    let mut lines: Vec<String> = Vec::new();
    // Crush source line behind each entry of `lines` (0 = none), for the
    // debugger's source map. Entries that carry no line are not pushed here;
    // `resize` below pads them.
    let mut line_src: Vec<usize> = Vec::new();
    let mut perms: HashSet<String> = HashSet::new();

    for _func in program.functions.values() {
        perms.extend(program.manifest.permissions.clone());
    }

    let is_kind = |name: &str| program.kind_table().is_some_and(|m| m.kind(name).is_some());
    // Lay the functions out in name order, not `HashMap` order: the same source
    // must assemble to byte-identical code — and a byte-identical disassembly —
    // on every run (the AOT build cache is keyed on it). CRUSH-188.
    let mut functions: Vec<_> = program.functions.iter().collect();
    functions.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (fname, func) in functions {
        // A GPU kernel (it uses `ptx_*` device ops, CRUSH-zorro-3) runs on the device via
        // `gpu.launch`, which compiles it from CASM with crush-ptx. It has no CVM1
        // form, so it is left out of the host program.
        if func.body.iter().any(|i| i.op.starts_with("ptx_")) {
            continue;
        }
        let mut slot_map: HashMap<String, u16> = HashMap::new();
        let mut next_slot: u16 = 0;
        let local_funcs: HashSet<String> = program.functions.keys().cloned().collect();

        lines.push(format!(".func {fname}"));

        let mut target_labels: HashMap<usize, String> = HashMap::new();
        for instr in func.body.iter() {
            if (instr.op == "jmp"
                || instr.op == "jmp_if"
                || instr.op == "jmp_if_not"
                || instr.op == "enter_try")
                && let Some(target) = instr.args.get("target").and_then(|v| v.as_u64())
            {
                target_labels
                    .entry(target as usize)
                    .or_insert_with(unique_label);
            }
        }

        for (i, instr) in func.body.iter().enumerate() {
            if let Some(label) = target_labels.get(&i) {
                lines.push(format!("{label}:"));
            }

            let op = match instr.op.as_str() {
                "push_int" => {
                    let v = instr.args["value"]
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("push_int missing value at {fname}:{i}"))?;
                    format!("PUSH {v}")
                }
                "push_float" => {
                    let v = instr.args["value"].as_f64().ok_or_else(|| {
                        anyhow::anyhow!("push_float missing value at {fname}:{i}")
                    })?;
                    format!("PUSH_F64 {v}")
                }
                "push_str" => {
                    let v = instr.args["value"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("push_str missing value at {fname}:{i}"))?;
                    format!("PUSH_STR {v:?}")
                }
                "push_bool" => {
                    let v = instr.args["value"]
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("push_bool missing value at {fname}:{i}"))?;
                    format!("PUSH_BOOL {}", if v { 1 } else { 0 })
                }
                "push_null" => "PUSH_NULL".to_string(),
                "pop" => "POP".to_string(),
                "dup" => "DUP".to_string(),
                "swap" => "SWAP".to_string(),
                "load" => {
                    let name = instr.args["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("load missing name at {fname}:{i}"))?;
                    let slot = get_slot(name, &mut slot_map, &mut next_slot);
                    format!("LOAD {slot}")
                }
                "store" => {
                    let name = instr.args["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("store missing name at {fname}:{i}"))?;
                    let slot = get_slot(name, &mut slot_map, &mut next_slot);
                    format!("STORE {slot}")
                }
                "add" => "ADD".to_string(),
                "sub" => "SUB".to_string(),
                "mul" => "MUL".to_string(),
                "div" => "DIV".to_string(),
                "mod" => "MOD".to_string(),
                "eq" => "EQ".to_string(),
                "ne" => "NE".to_string(),
                "lt" => "LT".to_string(),
                "gt" => "GT".to_string(),
                "le" => "LE".to_string(),
                "ge" => "GE".to_string(),
                "not" => "NOT".to_string(),
                "neg" => "NEG".to_string(),
                "and" => "AND".to_string(),
                "or" => "OR".to_string(),
                "call" => {
                    let fn_name = instr.args["function"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("call missing function at {fname}:{i}"))?;
                    let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0);
                    if local_funcs.contains(fn_name) {
                        format!("CALL {fn_name}")
                    } else {
                        perms.insert(fn_name.to_string());
                        format!("CAP_CALL {fn_name:?} {argc}")
                    }
                }
                // CRUSH-251: function values.
                "make_closure" => {
                    let fn_name = instr.args["function"].as_str().ok_or_else(|| {
                        anyhow::anyhow!("make_closure missing function at {fname}:{i}")
                    })?;
                    let arity = instr.args.get("arity").and_then(|v| v.as_u64()).unwrap_or(0);
                    let ncap = instr.args.get("ncap").and_then(|v| v.as_u64()).unwrap_or(0);
                    if !local_funcs.contains(fn_name) {
                        anyhow::bail!("make_closure over unknown function `{fn_name}` at {fname}:{i}");
                    }
                    format!("MAKE_CLOSURE {fn_name} {arity} {ncap}")
                }
                "call_value" => {
                    let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0);
                    format!("CALL_VALUE {argc}")
                }
                "call_value_apply" => "CALL_VALUE_APPLY".to_string(),
                // CRUSH-251 step 3: a variable closures capture by reference
                // lives in a cell in its slot.
                "cell_new" => "CELL_NEW".to_string(),
                "load_cell" | "store_cell" => {
                    let name = instr.args["name"].as_str().ok_or_else(|| {
                        anyhow::anyhow!("{} missing name at {fname}:{i}", instr.op)
                    })?;
                    let slot = get_slot(name, &mut slot_map, &mut next_slot);
                    let op = if instr.op == "load_cell" { "CELL_GET" } else { "CELL_SET" };
                    format!("LOAD {slot}\n{op}")
                }
                "cap_call" => {
                    let name = instr.args["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("cap_call missing name at {fname}:{i}"))?;
                    let argc = instr.args["argc"].as_u64().unwrap_or(0);
                    perms.insert(name.to_string());
                    format!("CAP_CALL {name:?} {argc}")
                }
                "ret" => "RET".to_string(),
                "jmp" => {
                    let target = instr.args["target"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("jmp missing target at {fname}:{i}"))?
                        as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("jmp to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("JMP {label}")
                }
                "jmp_if_not" => {
                    let target = instr.args["target"].as_u64().ok_or_else(|| {
                        anyhow::anyhow!("jmp_if_not missing target at {fname}:{i}")
                    })? as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("jmp_if_not to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("JZ {label}")
                }
                "jmp_if" => {
                    let target = instr.args["target"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("jmp_if missing target at {fname}:{i}"))?
                        as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("jmp_if to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("JNZ {label}")
                }
                // `size` on the `new_*` collection ops is the frontend's capacity
                // hint, not a count of operands on the stack (CVM1's NEW_* pops
                // that many), so every collection starts empty and the literal's
                // elements arrive through the matching `*_push`.
                "new_array" => "NEW_ARRAY 0".to_string(),
                "new_tuple" => "NEW_TUPLE 0".to_string(),
                "new_list" => "NEW_LIST 0".to_string(),
                "new_vector" => "NEW_VECTOR 0".to_string(),
                "new_set" => "NEW_SET 0".to_string(),
                "array_push" | "arr_push" => "ARR_PUSH".to_string(),
                "array_pop" | "arr_pop" => "ARR_POP".to_string(),
                "tuple_push" => "TUPLE_PUSH".to_string(),
                "list_push" => "LIST_PUSH".to_string(),
                "vector_push" => "VECTOR_PUSH".to_string(),
                "set_push" => "SET_PUSH".to_string(),
                "map_keys" => "MAP_KEYS".to_string(),
                "len" | "arr_len" => "ARR_LEN".to_string(),
                "bit_and" => "BITAND".to_string(),
                "bit_or" => "BITOR".to_string(),
                "bit_xor" => "BITXOR".to_string(),
                "bit_not" => "BITNOT".to_string(),
                "shl" => "SHL".to_string(),
                "shr" => "SHR".to_string(),
                // CVM1's ROT is [x, y, z] -> [y, x, z]; CASM's `rot` (and the
                // FastVM/JIT `Rot`) is [x, y, z] -> [y, z, x]. ROLL 2 is exactly
                // the latter, so don't lower to the divergent opcode.
                "rot" => "ROLL 2".to_string(),
                "pick" | "roll" => {
                    let n = instr.args.get("n").and_then(|v| v.as_u64()).ok_or_else(|| {
                        anyhow::anyhow!("{} missing n at {fname}:{i}", instr.op)
                    })?;
                    format!("{} {n}", instr.op.to_uppercase())
                }
                "type_of" => "TYPEOF".to_string(),
                "cast" => {
                    let ty = instr.args.get("type").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("cast missing type at {fname}:{i}")
                    })?;
                    format!("CAST {ty:?}")
                }
                "math_pow" => "MATH_POW".to_string(),
                "math_sqrt" => "MATH_SQRT".to_string(),
                "math_abs" => "MATH_ABS".to_string(),
                "math_round" => "MATH_ROUND".to_string(),
                "math_floor" => "MATH_FLOOR".to_string(),
                "math_ceil" => "MATH_CEIL".to_string(),
                "str_starts_with" => "STR_STARTS_WITH".to_string(),
                "str_ends_with" => "STR_ENDS_WITH".to_string(),
                "str_to_upper" => "STR_TO_UPPER".to_string(),
                "str_to_lower" => "STR_TO_LOWER".to_string(),
                "str_trim" => "STR_TRIM".to_string(),
                "index" | "arr_get" => "ARR_GET".to_string(),
                "make_range" => "MAKE_RANGE".to_string(),
                "str_contains" => "STR_CONTAINS".to_string(),
                "str_split" => "STR_SPLIT".to_string(),
                "str_replace" => "STR_REPLACE".to_string(),
                "str_join" => "STR_JOIN".to_string(),
                "arr_set" => "ARR_SET".to_string(),
                "export_var" => "NOP".to_string(),
                "exec_lang" => {
                    // Declare the block's polyglot grant: the VM checks EXEC_LANG against
                    // the manifest like any other capability (CRUSH-226).
                    if let Some(lang) = instr.args.get("lang").and_then(|v| v.as_str()) {
                        perms.insert(crush_vm::polyglot_gate_name(lang));
                    }
                    let args_json = serde_json::to_string(&instr.args).map_err(|e| {
                        anyhow::anyhow!("exec_lang: failed to serialize args at {fname}:{i}: {e}")
                    })?;
                    // Escape for assembly: \ → \\, " → \"
                    let esc = args_json.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("EXEC_LANG \"{esc}\"")
                }
                "spawn" => {
                    let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0);
                    format!("SPAWN {argc}")
                }
                "yield" => "YIELD".to_string(),
                "await" => "AWAIT".to_string(),
                "throw" => "THROW".to_string(),
                "enter_try" => {
                    let target = instr.args["target"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("enter_try missing target at {fname}:{i}"))?
                        as usize;
                    let label = target_labels.get(&target).ok_or_else(|| {
                        anyhow::anyhow!("enter_try to unknown target {target} at {fname}:{i}")
                    })?;
                    format!("ENTER_TRY {label}")
                }
                "halt" => "HALT".to_string(),
                "exit_try" => "EXIT_TRY".to_string(),
                // COSM K1.4: a struct literal is `new_obj` tagged with its
                // Kind (FastVM/JIT/AOT ignore the tag and build a map); the
                // VMs build a Record (no constructor runs: a literal writes
                // the fields itself).
                "new_obj" => match instr.args.get("kind").and_then(|v| v.as_str()) {
                    Some(kind) if is_kind(kind) => format!("NEW_RECORD {kind:?}"),
                    _ => "NEW_OBJ".to_string(),
                },
                "get_field" => {
                    let field = instr
                        .args
                        .get("field")
                        .or_else(|| instr.args.get("name"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("get_field missing field arg at {fname}:{i}")
                        })?;
                    format!("GET_FIELD {field:?}")
                }
                "set_field" => {
                    let field = instr
                        .args
                        .get("field")
                        .or_else(|| instr.args.get("name"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("set_field missing field arg at {fname}:{i}")
                        })?;
                    format!("SET_FIELD {field:?}")
                }
                // `new P()`: an instance of the Kind `P` (COSM K1.4); a name
                // that isn't a Kind still builds a plain object.
                "new_struct" => match instr.args.get("name").and_then(|v| v.as_str()) {
                    Some(kind) if is_kind(kind) => construct_asm(program, kind, 0),
                    _ => "NEW_OBJ".to_string(),
                },
                // COSM K1.4 (CRUSH-zorro-43): the generic member ops.
                "get_member" | "set_member" | "has_member" | "del_member" => {
                    let name = instr.args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("{} missing name at {fname}:{i}", instr.op)
                    })?;
                    format!("{} {name:?}", instr.op.to_uppercase())
                }
                "call_member" => {
                    let name = instr.args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("call_member missing name at {fname}:{i}")
                    })?;
                    let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0);
                    format!("CALL_MEMBER {name:?} {argc}")
                }
                "construct" => {
                    let kind = instr.args.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("construct missing kind at {fname}:{i}")
                    })?;
                    let argc = instr.args.get("argc").and_then(|v| v.as_u64()).unwrap_or(0);
                    construct_asm(program, kind, argc)
                }
                "get_slot" | "set_slot" => {
                    let slot = instr.args.get("slot").and_then(|v| v.as_u64()).ok_or_else(|| {
                        anyhow::anyhow!("{} missing slot at {fname}:{i}", instr.op)
                    })?;
                    format!("{} {slot}", instr.op.to_uppercase())
                }
                "is_kind" => {
                    let kind = instr.args.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("is_kind missing kind at {fname}:{i}")
                    })?;
                    format!("IS_KIND {kind:?}")
                }
                "load_kind" => {
                    let kind = instr.args.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("load_kind missing kind at {fname}:{i}")
                    })?;
                    format!("LOAD_KIND {kind:?}")
                }
                "copy" => "COPY".to_string(),
                // Stubs for unimplemented opcodes — prevent bailing
                "dom_mutate" | "dom_event_listener" | "dom_query" => "NOP".to_string(),
                // CRUSH-156: the ten `ai_native.*` kinds lower to their AI
                // opcode with the compiled payload as the string operand
                // (`crush_vm::ai_args` is the contract). Statement forms
                // push the cap's result like the expression forms, so a POP
                // follows to keep the stack balanced.
                "ai_query"
                | "ai_tool_chain"
                | "ai_toolchain"
                | "ai_agent_delegation"
                | "ai_learning_loop"
                | "ai_context_aware"
                | "ai_synthesize"
                | "ai_semantic_match"
                | "ai_goal_decl"
                | "ai_goal_declaration"
                | "ai_progress_update"
                | "ai_knowledge_share"
                | "ai_knowledge_sharing" => {
                    let (opcode, statement) = match instr.op.as_str() {
                        "ai_query" => ("AI_QUERY", false),
                        "ai_tool_chain" | "ai_toolchain" => ("AI_TOOLCHAIN", false),
                        "ai_agent_delegation" => ("AI_AGENT_DELEGATION", false),
                        "ai_learning_loop" => ("AI_LEARNING_LOOP", false),
                        "ai_context_aware" => ("AI_CONTEXT_AWARE", false),
                        "ai_synthesize" => ("AI_SYNTHESIZE", false),
                        "ai_semantic_match" => ("AI_SEMANTIC_MATCH", false),
                        "ai_goal_decl" | "ai_goal_declaration" => ("AI_GOAL_DECLARATION", true),
                        "ai_progress_update" => ("AI_PROGRESS_UPDATE", true),
                        _ => ("AI_KNOWLEDGE_SHARING", true),
                    };
                    let payload = serde_json::to_string(&instr.args).map_err(|e| {
                        anyhow::anyhow!(
                            "{}: failed to serialize args at {fname}:{i}: {e}",
                            instr.op
                        )
                    })?;
                    if statement {
                        format!("{opcode} {payload:?}\n    POP")
                    } else {
                        format!("{opcode} {payload:?}")
                    }
                }
                // CRUSH-265: `ai_capability_discovery`, `ai_adaptation_request` and
                // `ai_semantic_switch` used to lower to `NOP` here — a silent no-op.
                // They now fail loudly (see the frontend's `compile_ai_statement` and
                // the `Unsupported CVM1 opcode` arm below) instead of being silently
                // dropped; porting onto `ai_native` is a later ticket.
                other => anyhow::bail!("Unsupported CVM1 opcode: {other} at {fname}:{i}"),
            };
            line_src.resize(lines.len(), 0);
            lines.push(format!("    {op}"));
            line_src.push(
                instr
                    .meta
                    .as_ref()
                    .and_then(|m| m.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as usize,
            );
        }

        // Emit label for one-past-the-end targets (used by try/catch's jmp with
        // an end_label = instrs.len()). The instruction loop above only visits
        // indices 0..len-1, so labels targeting index `len` would never be emitted.
        if let Some(label) = target_labels.get(&func.body.len()) {
            lines.push(format!("{label}:"));
        }
        let mut names: Vec<(String, u16)> = slot_map.into_iter().collect();
        names.sort_by_key(|&(_, slot)| slot);
        local_names.insert(fname.clone(), names);
    }

    // Post-process: restore the frontend's stack discipline around a
    // non-returning CAP_CALL. The frontend compiles every expression as
    // leaving exactly one value, but a void capability (`io.print`, ...)
    // pushes nothing. When the very next line is the statement's POP, drop
    // it; otherwise the value is consumed somewhere else (a match arm's
    // `JMP` to a shared POP, a STORE, a RET), so push the null the
    // expression stands for. Only dropping an adjacent POP underflowed the
    // stack when a void call ended a match arm (GitHub issue #143).
    line_src.resize(lines.len(), 0);
    let mut cleaned: Vec<String> = Vec::new();
    // Crush source line of each physical line of the assembly (index = line - 1).
    let mut asm_src: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        cleaned.push(line.clone());
        asm_src.extend(std::iter::repeat_n(
            line_src[i],
            line.matches('\n').count() + 1,
        ));
        if let Some(cap_name) = line.trim().strip_prefix("CAP_CALL ") {
            let name = cap_name.split('"').nth(1).unwrap_or("");
            if !cap_returns_value(name) {
                if lines.get(i + 1).map(|l| l.trim()) == Some("POP") {
                    i += 1;
                } else {
                    cleaned.push("    PUSH_NULL".to_string());
                    asm_src.push(line_src[i]);
                }
            }
        }
        i += 1;
    }

    let assembly = cleaned.join("\n");
    let perms_slice: Vec<&str> = perms.iter().map(|s| s.as_str()).collect();
    let mut vm_program = crush_vm::assemble(
        &assembly,
        if perms_slice.is_empty() {
            None
        } else {
            Some(&perms_slice)
        },
        None,
    )?;
    // COSM K1.3 (CRUSH-zorro-42): the kind table goes into the CVM1 program
    // as-is; `to_blob` writes it as the version-3 kind-table section.
    vm_program.cosm = program.cosm.clone();
    // The assembler maps offsets to lines of the generated assembly; the
    // debugger wants the author's lines, which the instructions carry in their
    // `meta`. Instructions without one drop out of the map.
    vm_program.source_map = vm_program
        .source_map
        .iter()
        .filter_map(|&(asm_line, offset)| {
            let line = *asm_src.get(asm_line.checked_sub(1)?)?;
            (line > 0).then_some((line, offset))
        })
        .collect();
    // COSM K1.4: a program with Kinds records each function's arity, which
    // the VMs need to make function values (bound methods) from members.
    if vm_program.kind_table().is_some() {
        for (name, entry) in vm_program.manifest.functions.iter_mut() {
            entry.arity = program.functions.get(name).and_then(|f| u8::try_from(f.params.len()).ok());
        }
    }
    Ok((vm_program, local_names))
}

/// `CONSTRUCT "kind" argc`, followed — for a Kind built by an initialiser
/// function — by the `POP` of that function's return value the VMs expect
/// (`crush_vm::cosm_rt::initialises_by_call`).
fn construct_asm(program: &casm::Program, kind: &str, argc: u64) -> String {
    let calls = program.kind_table().is_some_and(|module| crush_vm::cosm_rt::initialises_by_call(module, kind));
    if calls {
        format!("CONSTRUCT {kind:?} {argc}\n    POP")
    } else {
        format!("CONSTRUCT {kind:?} {argc}")
    }
}

#[cfg(test)]
mod tests {
    /// CRUSH-317: the program's source map is in the author's lines, so a
    /// debugger can break on `file:line`; the local names ride along.
    #[test]
    fn compiled_crush_maps_offsets_to_source_lines_and_names_its_locals() {
        let src = "fn main() {\n    let a = 1\n    let b = a + 2\n    print(b)\n}\nmain()\n";
        let (program, names) = compile_crush_source_for_debug(src).unwrap();
        let lines: std::collections::BTreeSet<usize> =
            program.source_map.iter().map(|&(l, _)| l).collect();
        assert!(lines.is_superset(&[2, 3, 4].into()), "{lines:?}");
        assert!(lines.iter().all(|&l| (1..=6).contains(&l)), "{lines:?}");
        // Offsets are code offsets, ascending, inside the program.
        assert!(program.source_map.windows(2).all(|w| w[0].1 <= w[1].1));
        assert!(
            program
                .source_map
                .iter()
                .all(|&(_, o)| o < program.code.len())
        );
        let main: Vec<&str> = names["main"].iter().map(|(n, _)| n.as_str()).collect();
        assert!(main.contains(&"a") && main.contains(&"b"), "{main:?}");
    }

    fn _poly_caps() -> crush_vm::HostCaps {
        let mut c = crush_vm::HostCaps::new();
        c.grant_polyglot(&["python", "javascript", "bash"]);
        c
    }

    /// The `@python` polyglot tests spawn a real `python3`; skip them when it
    /// isn't on PATH (same convention as the AOT backends' `gcc`/`cc` gates).
    fn python3_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    use super::*;

    // CRUSH-188: the same source must assemble to byte-identical code. The
    // function layout used to follow `HashMap` iteration order, so two runs
    // produced different CASM/disassembly.
    #[test]
    fn casm_to_vm_lays_out_functions_in_name_order() {
        let src = "fn zeta(n) { return n + 1 }\n\
                   fn alpha(n) { return zeta(n) * 2 }\n\
                   fn main() { print(alpha(5)) }\n";
        let text = crush_vm::disassemble(&casm_to_vm(&compile_crush_to_casm(src).unwrap()).unwrap());
        let order: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix(".func "))
            .map(|l| l.split_whitespace().next().unwrap())
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "functions must be laid out in name order:\n{text}");

        // Two independent compilations agree byte for byte.
        let a = crush_vm::disassemble(&casm_to_vm(&compile_crush_to_casm(src).unwrap()).unwrap());
        let b = crush_vm::disassemble(&casm_to_vm(&compile_crush_to_casm(src).unwrap()).unwrap());
        assert_eq!(a, b, "the same source must assemble identically");
    }

    #[test]
    fn test_compile_simple_program() {
        let source = "fn main() {\n    io.print(\"hello from crush\")\n}\n";
        let program = compile_crush_source(source).expect("compilation failed");
        assert!(!program.code.is_empty(), "should produce bytecode");
        assert!(!program.consts.is_empty(), "should have constants");
    }

    #[test]
    fn test_compile_with_expression() {
        let source = "fn main() {\n    let x = 42\n    io.print(x)\n}\n";
        let program = compile_crush_source(source).expect("compilation failed");
        assert!(!program.code.is_empty());
    }

    #[test]
    fn test_compile_and_run() {
        let source = "fn main() {\n    io.print(\"hello from crush\")\n}\n";
        let prog = compile_crush_source(source).expect("compile");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run");
        assert_eq!(result.output, "hello from crush\n");
        assert!(result.halted);
    }

    #[test]
    fn test_compile_with_bool() {
        let source = "fn main() {\n    let a = true\n    let b = false\n    io.print(a)\n}\n";
        let prog = compile_crush_source(source).expect("compile bool");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run bool");
        assert_eq!(result.output, "true\n");
    }

    #[test]
    fn test_compile_with_if_bool_condition() {
        let source = "fn main() {\n    if true {\n        io.print(\"yes\")\n    } else {\n        io.print(\"no\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile if bool");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run if bool");
        assert_eq!(result.output, "yes\n");
    }

    #[cfg(feature = "native-plugins")]
    #[test]
    fn test_compiled_for_loop_array_accumulator_runs_in_fastvm() {
        let source = r#"
            fn main() {
                let acc = [];
                for i in 0..3 {
                    acc = acc.push(i);
                }
                return len(acc);
            }
        "#;
        let casm = crush_frontend::compile_crush_source(source).expect("compile for loop");
        let lowered = crush_vm::fastvm::lower_program(&casm).expect("lower for loop");
        let mut vm = crush_vm::fastvm::FastVM::new(
            lowered,
            Vec::new(),
            std::sync::Arc::new(crate::compile::tests::TestHal),
        );
        assert_eq!(
            vm.run(10_000),
            crush_vm::fastvm::FastYield::Finished(Some(crush_vm::RuntimeValue::Int(3)))
        );
    }

    #[cfg(feature = "native-plugins")]
    #[test]
    fn test_compiled_array_for_loop_runs_in_fastvm() {
        let source = r#"
            fn main() {
                let acc = [];
                for item in [1, 2, 3] {
                    if item == 2 {
                        continue;
                    }
                    acc = acc.push(item);
                }
                return len(acc);
            }
        "#;
        let casm = crush_frontend::compile_crush_source(source).expect("compile array loop");
        let lowered = crush_vm::fastvm::lower_program(&casm).expect("lower array loop");
        let mut vm = crush_vm::fastvm::FastVM::new(
            lowered,
            Vec::new(),
            std::sync::Arc::new(TestHal),
        );
        assert_eq!(
            vm.run(10_000),
            crush_vm::fastvm::FastYield::Finished(Some(crush_vm::RuntimeValue::Int(2)))
        );
    }

    #[cfg(feature = "native-plugins")]
    #[test]
    fn test_compiled_for_loop_arithmetic_assignment_runs_in_fastvm() {
        let source = r#"
            fn main() {
                let total = 0;
                for item in [1, 2, 3] {
                    total = total + item;
                }
                return total;
            }
        "#;
        let casm = crush_frontend::compile_crush_source(source).expect("compile arithmetic loop");
        let lowered = crush_vm::fastvm::lower_program(&casm).expect("lower arithmetic loop");
        let mut vm = crush_vm::fastvm::FastVM::new(
            lowered,
            Vec::new(),
            std::sync::Arc::new(TestHal),
        );
        assert_eq!(
            vm.run(10_000),
            crush_vm::fastvm::FastYield::Finished(Some(crush_vm::RuntimeValue::Int(6)))
        );
    }

    #[cfg(feature = "native-plugins")]
    #[derive(Debug)]
    struct TestHal;

    #[cfg(feature = "native-plugins")]
    impl crush_vm::fastvm::Hal for TestHal {}

    #[test]
    fn test_compile_with_object() {
        let source = "fn main() {\n    let obj = {name: \"crush\", version: 42}\n    io.print(obj.name)\n}\n";
        let prog = compile_crush_source(source).expect("compile object");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run object");
        assert_eq!(result.output, "crush\n");
    }

    #[test]
    fn test_compile_with_try_catch() {
        let source = "fn main() {\n    try {\n        io.print(\"in try\")\n    } catch err {\n        io.print(\"in catch\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile try/catch");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run try/catch");
        assert_eq!(result.output, "in try\n");
    }

    #[test]
    fn test_compile_with_throw_and_catch() {
        let source = "fn main() {\n    try {\n        throw \"error!\"\n        io.print(\"not reached\")\n    } catch err {\n        io.print(\"caught\")\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile throw/catch");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, None).expect("run throw/catch");
        assert_eq!(result.output, "caught\n");
    }

    // Regression: a `@javascript { ... }` polyglot block used to fail at
    // CASM assembly time (the source's `;` broke the assembler's comment
    // stripper — see crush_vm::assembler tests) before ever reaching the
    // language executor, and even once it parsed, EXEC_LANG ran `node -c`
    // (syntax-check-only) instead of `node -e` (execute). Requires `node`
    // on PATH.
    #[test]
    fn test_javascript_polyglot_block_executes() {
        let source = "fn main() {\n    @javascript { const x = 1; }\n    io.print(\"js ok\")\n}\n";
        let prog = compile_crush_source(source).expect("compile js polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run js polyglot block");
        assert_eq!(result.output, "js ok\n");
    }

    // An `@<lang>` block for a language with no registered executor must
    // fail loudly and name the language — never silently no-op.
    #[test]
    fn test_unregistered_polyglot_language_errors_loudly() {
        let source = "fn main() {\n    @cobol { DISPLAY \"hi\". }\n}\n";
        let prog = compile_crush_source(source).expect("compile unregistered-lang block");
        let quotas = crush_vm::Quotas::default();
        let err = crush_vm::run_with_caps(&prog, &quotas, None)
            .expect_err("unregistered language must not silently succeed");
        let msg = err.to_string();
        assert!(
            msg.contains("no executor registered for language 'cobol'"),
            "error should name the unregistered language, got: {msg}"
        );
    }

    // A hung/slow polyglot subprocess must be killed at
    // `Quotas::max_wall_time_ms`, not allowed to block the interpreter
    // indefinitely — `max_steps` never trips for a process blocked on I/O,
    // since it isn't executing crush instructions while it hangs. The real
    // proof isn't just the error variant; it's that this test finishes in
    // well under the process's 30s sleep, showing the subprocess was
    // actually killed rather than eventually finishing on its own.
    #[test]
    fn test_exec_lang_wall_clock_timeout_kills_hung_subprocess() {
        let source = "fn main() {\n    @bash {\n        sleep 30\n    }\n}\n";
        let prog = compile_crush_source(source).expect("compile bash polyglot block");
        let quotas = crush_vm::Quotas {
            max_wall_time_ms: 200,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let err = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect_err("a hung subprocess must be killed, not silently allowed to keep running");
        let elapsed = start.elapsed();
        let msg = err.to_string();
        assert!(
            msg.contains("wall-clock quota") && msg.contains("killed") && msg.contains("polyglot.bash"),
            "error should name the capability and say it was killed, got: {msg}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "the 30s sleep must be KILLED at the 200ms quota, not waited out — took {elapsed:?}"
        );
    }

    // End-to-end: a Crush local flows into a `@python` block (input
    // marshaling) and the block's own bound name flows back out (output
    // marshaling), entirely via the AST free-variable analysis — no
    // explicit declaration syntax needed. Requires `python3` on PATH.
    #[test]
    fn test_python_polyglot_block_marshals_input_and_output() {
        let source =
            "fn main() {\n    let base = 5;\n    @python {\n        result = base * 2\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run python polyglot block");
        assert_eq!(result.output, "10\n");
    }

    // Same, but multi-line with an import and a non-integer result — the
    // real shape of crush-website's example.crush.
    #[test]
    fn test_python_polyglot_block_marshals_float_result_with_import() {
        let source = "fn main() {\n    let base = 5;\n    @python {\n        import math\n        result = math.pow(base, 3)\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run python polyglot block");
        // JSON round-trip preserves float-ness (Python's json.dumps(125.0)
        // stays "125.0"), not just the numeric value.
        assert_eq!(result.output, "125.0\n");
    }

    // The block's own ordinary prints must keep flowing through as normal
    // program output — only the sentinel-marked line is the marshaled
    // return value, never the whole stdout stream.
    #[test]
    fn test_python_polyglot_block_own_prints_still_visible() {
        let source = "fn main() {\n    let base = 5;\n    @python {\n        print(\"debug\")\n        result = base + 1\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run python polyglot block");
        assert!(
            result.output.contains("debug"),
            "block's own print output should still appear, got: {}",
            result.output
        );
        assert!(
            result.output.contains('6'),
            "marshaled result should still appear, got: {}",
            result.output
        );
    }

    // CRUSH-68: string inject used to be Display (`hello`), which is not
    // valid JSON — Python's json.loads crashed. Must round-trip as `"hello"`.
    #[test]
    fn test_python_polyglot_marshals_string_input() {
        let source = "fn main() {\n    let msg = \"hello\";\n    @python {\n        result = msg + \"!\"\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run");
        assert_eq!(result.output, "hello!\n");
    }

    // CRUSH-68: array inject must be JSON, not Display `[1, 2]`.
    #[test]
    fn test_python_polyglot_marshals_array_input() {
        let source = "fn main() {\n    let xs = [1, 2, 3];\n    @python {\n        result = xs[0] + xs[2]\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run");
        assert_eq!(result.output, "4\n");
    }

    // CRUSH-68: map inject.
    #[test]
    fn test_python_polyglot_marshals_map_input() {
        let source = "fn main() {\n    let obj = {a: 10, b: 20};\n    @python {\n        result = obj[\"a\"] + obj[\"b\"]\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run");
        assert_eq!(result.output, "30\n");
    }

    // issue #225: a Crush variable read only inside a Python comprehension
    // was never detected as an input, so the comprehension raised NameError.
    // This is the exact repro from the issue.
    #[test]
    fn test_python_polyglot_issue_225_comprehension_reads_injected_variable() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let source = "fn main() {\n    let n = 7\n    @python {\n        import math\n        nums = [math.factorial(i) for i in range(1, n + 1)]\n        result = nums\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect("run python polyglot block");
        assert_eq!(result.output, "[1, 2, 6, 24, 120, 720, 5040]\n");
    }

    // issue #225: the same free-variable gap bit generator expressions.
    #[test]
    fn test_python_polyglot_issue_225_generator_expression_reads_injected_variable() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let source = "fn main() {\n    let n = 5\n    @python {\n        result = sum(i * i for i in range(1, n + 1))\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect("run python polyglot block");
        assert_eq!(result.output, "55\n");
    }

    // issue #225: lambda bodies close over the enclosing (module) scope, so a
    // Crush variable read inside one must be injected.
    #[test]
    fn test_python_polyglot_issue_225_lambda_reads_injected_variable() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let source = "fn main() {\n    let k = 3\n    @python {\n        f = lambda x: x + k\n        result = f(10)\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect("run python polyglot block");
        assert_eq!(result.output, "13\n");
    }

    // issue #225: nested `def` bodies are walked for free reads (minus their
    // parameters and locals).
    #[test]
    fn test_python_polyglot_issue_225_nested_def_reads_injected_variable() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let source = "fn main() {\n    let k = 3\n    @python {\n        def add(x):\n            return x + k\n        result = add(10)\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect("run python polyglot block");
        assert_eq!(result.output, "13\n");
    }

    // issue #225: reads and bindings inside `try`/`except` bodies are now
    // analyzed (a Crush variable read in the `try` body is injected, and a
    // name bound there isn't treated as a free read later).
    #[test]
    fn test_python_polyglot_issue_225_try_except_reads_injected_variable() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let source = "fn main() {\n    let n = 7\n    @python {\n        try:\n            x = n + 1\n        except Exception:\n            x = -1\n        result = x\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile python polyglot block");
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps()))
            .expect("run python polyglot block");
        assert_eq!(result.output, "8\n");
    }

    // CRUSH-68: JS gets the same typed marshal protocol as Python.
    #[test]
    fn test_javascript_polyglot_marshals_string_input() {
        let source = "fn main() {\n    let msg = \"hello\";\n    @javascript {\n        let result = msg + \"!\";\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(source).expect("compile js marshal");
        let quotas = crush_vm::Quotas::default();
        let result =
            crush_vm::run_with_caps(&prog, &quotas, Some(&_poly_caps())).expect("run js marshal");
        assert_eq!(result.output, "hello!\n");
    }

    #[test]
    #[cfg(any(feature = "polyglot-python", feature = "polyglot-javascript"))]
    fn test_dedent_strips_common_indent_only() {
        assert_eq!(dedent("    a\n    b\n"), "a\nb");
        assert_eq!(dedent("  a\n    b\n"), "a\n  b");
        assert_eq!(dedent("a"), "a");
    }

    /// CRUSH-226: a compiled `@lang` block declares its gate, so the VM's
    /// declared-caps check passes for anything this compiler produced.
    #[test]
    fn test_compiled_lang_block_declares_its_polyglot_gate() {
        let prog = compile_crush_source("@py {\nx = 1\n}\n@javascript {\nlet y = 2\n}\n").expect("compile");
        let perms = &prog.manifest.permissions;
        assert!(perms.iter().any(|p| p == "polyglot.python"), "{perms:?}");
        assert!(perms.iter().any(|p| p == "polyglot.javascript"), "{perms:?}");
    }

    /// Run on the scheduler (`run_with_caps`) AND on `PortableVm`; both must
    /// print the same thing.
    fn run_both_engines(source: &str) -> String {
        let prog = compile_crush_source(source).expect("compile");
        let sched = crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&_poly_caps()))
            .expect("scheduler run")
            .output;
        let mut vm = crush_vm::PortableVm::new(prog);
        vm.set_host_caps(_poly_caps());
        let portable = vm.run().expect("PortableVm run").output;
        assert_eq!(sched, portable, "engines disagree");
        sched
    }

    // CRUSH-224: `use @lang` reaches later blocks. The ticket's repro: the
    // module is bound under its own name, the alias in addition.
    #[test]
    fn test_use_lang_python_module_and_alias_reach_block() {
        let out = run_both_engines(
            "use @lang python \"math\" as m\n@python {\nprint(\"sqrt16 =\", math.sqrt(16), m.floor(2.5))\n}\n",
        );
        assert_eq!(out, "sqrt16 = 4.0 2");
    }

    #[test]
    fn test_use_lang_python_selective_import() {
        let out = run_both_engines(
            "use @lang python \"os.path\" { \"join\" }\nfn main() {\n    let a = \"x\";\n    @python {\n        r = join(a, \"y\")\n    }\n    print(r);\n}\n",
        );
        assert_eq!(out, "x/y\n");
    }

    #[test]
    fn test_use_lang_javascript_require() {
        let out = run_both_engines(
            "use @lang javascript \"path\" as p\n@javascript {\nconsole.log(p.basename(\"/a/b.txt\"), path.sep)\n}\n",
        );
        assert_eq!(out, "b.txt /");
    }

    // A file-level `use` reaches blocks in other functions; a `use` only
    // reaches blocks of its own language.
    #[test]
    fn test_use_lang_reaches_other_functions_and_only_its_language() {
        let src = "use @lang python \"json\"\nfn helper() {\n    @python {\n        print(json.dumps([1]))\n    }\n}\nfn main() {\n    helper();\n}\n";
        assert_eq!(run_both_engines(src), "[1]");
        let mut program = crush_frontend::parse_source(
            "use @lang python \"json\"\n@javascript {\nlet z = 1\n}\n",
        )
        .expect("parse");
        prepare_polyglot_blocks(&mut program).expect("prepare");
        let has_imports = program.functions["main"].body.iter().any(|s| {
            matches!(s, crush_cast::Statement::LangBlock { imports, .. } if !imports.is_empty())
        });
        assert!(!has_imports, "a python import must not reach a javascript block");
    }

    // The import goes on guest line 1, so guest line K is still .crush line
    // block_line + K - 1 — with and without marshaling.
    #[test]
    fn test_use_lang_keeps_guest_line_numbers() {
        for src in [
            "use @lang python \"math\"\n@python {\nx = math.pi\nboom()\n}\n",
            "use @lang python \"math\"\nlet v = 1\n@python {\nx = math.pi + v\nboom()\n}\n",
        ] {
            let prog = compile_crush_source(src).expect("compile");
            let err = crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&_poly_caps()))
                .expect_err("boom() is undefined");
            let msg = err.to_string();
            // block on .crush line 2 or 3; `boom()` is guest line 3.
            assert!(msg.contains("line 3, in <module>"), "{msg}");
        }
    }

    #[test]
    fn test_use_lang_bash_is_a_compile_error() {
        let err = compile_crush_source("use @lang bash \"lib.sh\"\n@bash {\necho hi\n}\n")
            .expect_err("bash imports are rejected");
        assert!(format!("{err:#}").contains("bash"), "{err:#}");
    }

    // The `use` itself no longer lowers to a throwaway exec_lang.
    #[test]
    fn test_use_lang_emits_no_exec_lang_of_its_own() {
        let prog = compile_crush_source("use @lang python \"math\"\nlet x = 1\n").expect("compile");
        assert!(!prog.manifest.permissions.iter().any(|p| p.starts_with("polyglot.")));
    }

    // ── CRUSH-270: explicit `->` outputs ──────────────────────────────────

    // #230's two blocks, migrated to `->`: only the declared name crosses, so
    // block 2 reads block 1's `secret` through a real Crush variable.
    #[test]
    fn test_crush_270_explicit_output_only_declared_names_cross() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    @python -> secret {\n        secret = 42\n    }\n    @python -> result {\n        result = secret + 1\n    }\n    print(result);\n}\n";
        assert_eq!(run_both_engines(src), "43\n");
    }

    // Without `->`, the old implicit rule still runs — and warns, naming the
    // exported variable and suggesting the `->` form.
    #[test]
    fn test_crush_270_implicit_rule_still_works_but_warns() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    @python {\n        result = 5 * 8\n    }\n    print(result);\n}\n";
        let prog = compile_crush_source(src).expect("compile");
        let out = crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&_poly_caps()))
            .expect("run");
        assert_eq!(out.output, "40\n");
        let warn = prog.warnings.join("\n");
        assert!(
            warn.contains("`result`") && warn.contains("-> result"),
            "expected an implicit-output warning naming `result`, got: {warn}"
        );
    }

    // `-> result` lets the block export the intended (non-last) assignment —
    // today's implicit rule would have exported `tmp` and lost `result`.
    #[test]
    fn test_crush_270_explicit_output_keeps_intermediate() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    @python -> result {\n        result = 5 * 8\n        tmp = result + 1\n    }\n    print(result);\n}\n";
        assert_eq!(run_both_engines(src), "40\n");
    }

    // The clobber case: `-> total` (not `count`) leaves the Crush `count`
    // untouched, and the block's own `count = 0` stays inside the block.
    #[test]
    fn test_crush_270_explicit_output_does_not_clobber() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    let count = 10\n    @python -> total {\n        count = 0\n        total = count * 2\n    }\n    print(count);\n    print(total);\n}\n";
        assert_eq!(run_both_engines(src), "10\n0\n");
    }

    // Without `->`, the implicit export of `count` overwrites the Crush
    // variable, and the compiler warns about the clobber.
    #[test]
    fn test_crush_270_implicit_clobber_warns() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    let count = 10\n    @python {\n        count = 0\n    }\n    print(count);\n}\n";
        let prog = compile_crush_source(src).expect("compile");
        let out = crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&_poly_caps()))
            .expect("run");
        assert_eq!(out.output, "0\n");
        let warn = prog.warnings.join("\n");
        assert!(
            warn.contains("overwrites") && warn.contains("`count`"),
            "expected a clobber warning naming `count`, got: {warn}"
        );
    }

    // The env-only fallback is gone: a Crush variable the block never reads by
    // name (only via `dir()`/`globals()`) is no longer injected — not by name
    // and not via `os.environ` — while one the block *reads* is injected by
    // name.
    #[test]
    fn test_crush_270_env_fallback_removed() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    let n = 5\n    @python -> has_n {\n        import os\n        has_n = (\"n\" in os.environ) or (\"n\" in globals())\n    }\n    print(has_n);\n    @python -> r {\n        r = n * 2\n    }\n    print(r);\n}\n";
        assert_eq!(run_both_engines(src), "false\n10\n");
    }

    // Multi-output `-> (a, b)`.
    #[test]
    fn test_crush_270_multi_output() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    @python -> (mean, stdev) {\n        mean = 5\n        stdev = 2\n    }\n    print(mean);\n    print(stdev);\n}\n";
        assert_eq!(run_both_engines(src), "5\n2\n");
    }

    // A declared-but-unbound output is a compile error naming the variable.
    #[test]
    fn test_crush_270_declared_but_unbound_is_compile_error() {
        let src = "fn main() {\n    @python -> nope {\n        x = 1\n    }\n    print(nope);\n}\n";
        let err = compile_crush_source(src).expect_err("declared output `nope` is never bound");
        assert!(err.to_string().contains("`nope`"), "error should name `nope`: {err}");
    }

    // A conditionally-bound output that misses at run time is a loud error
    // naming the variable — never marshaled as `null`.
    #[test]
    fn test_crush_270_conditional_output_miss_is_loud() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "fn main() {\n    @python -> x {\n        if False:\n            x = 1\n    }\n    print(x);\n}\n";
        let prog = compile_crush_source(src).expect("conditional binding compiles");
        let err = crush_vm::run_with_caps(&prog, &crush_vm::Quotas::default(), Some(&_poly_caps()))
            .expect_err("a missing conditional output must error");
        let msg = err.to_string();
        assert!(
            msg.contains("'x'") || msg.contains("`x`"),
            "error should name the missing output `x`, got: {msg}"
        );
    }

    // `->` on a language with no value marshaling is a loud compile error.
    #[test]
    fn test_crush_270_arrow_on_bash_is_rejected() {
        let src = "fn main() {\n    @bash -> x {\n        echo hi\n    }\n}\n";
        let err = compile_crush_source(src).expect_err("bash has no value marshaling");
        assert!(err.to_string().contains("@bash"), "error should name the language: {err}");
    }

    // #255: @bash blocks see the Crush variables they reference via
    // $name / ${name} as env vars (restores the pre-#245 behavior).
    #[test]
    fn test_bash_var_refs_scanner() {
        // Basic $name and ${name} forms
        assert_eq!(
            bash_var_refs("echo $name"),
            vec!["name".to_string()]
        );
        assert_eq!(
            bash_var_refs("echo ${name}"),
            vec!["name".to_string()]
        );
        // Dedupes, preserves first-seen order
        assert_eq!(
            bash_var_refs("echo $a $b ${a}"),
            vec!["a".to_string(), "b".to_string()]
        );
        // Skips single-quoted (no expansion), $$ (PID), $1 (positional)
        assert_eq!(bash_var_refs("echo '$name' $$ $1"), Vec::<String>::new());
        // ${name:-default} counts as a reference to name
        assert_eq!(
            bash_var_refs("echo ${name:-fallback}"),
            vec!["name".to_string()]
        );
    }

    // #255 end-to-end: a referenced Crush var reaches the bash block's
    // exec_lang spec in the compiled CASM.
    #[test]
    fn test_bash_block_sees_referenced_crush_var() {
        let src = "fn main() {\n    let name = \"crush\"\n    @bash {\n        echo \"hello [$name]\"\n    }\n}\n";
        let casm = compile_crush_to_casm(src).expect("compile bash block");
        let json = serde_json::to_string(&casm).expect("casm to json");
        assert!(
            json.contains("\"var_0\":\"name\"") || json.contains("\"var_0\": \"name\""),
            "bash block's exec_lang spec should list `name` as var_0: {json}"
        );
    }

    // `use @lang` + `->` still splice the import into the block.
    #[test]
    fn test_crush_270_use_lang_with_output() {
        if !python3_available() {
            eprintln!("python3 not found; skipping polyglot test");
            return;
        }
        let src = "use @lang python \"math\"\nfn main() {\n    @python -> r {\n        r = math.floor(2.7)\n    }\n    print(r);\n}\n";
        assert_eq!(run_both_engines(src), "2\n");
    }
}

#[cfg(all(
    test,
    not(any(
        feature = "polyglot-python",
        feature = "polyglot-javascript",
        feature = "polyglot-rust"
    ))
))]
mod source_dispatch_without_adapters {
    #[test]
    fn rust_is_rejected_and_crush_remains_available() {
        let error = super::compile_source("rust", "fn main() {}").unwrap_err();
        assert!(
            error.to_string().contains("no walker adapters enabled"),
            "{error}"
        );
        let program = super::compile_source("crush", "fn main() { return 42; }").unwrap();
        assert_eq!(
            crate::Runtime::new().run(&program).unwrap().return_value,
            Some(crate::Value::Int(42))
        );
    }
}
