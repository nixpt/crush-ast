//! `crushc` — the Crush compiler, akin to `rustc`.
//!
//! Compiles `.crush` source — or CAST (`.cast.json`, `.cast`, `.castb`) — to
//! CVM1 bytecode (`.cvm1`), or emits another representation of it. Every
//! format is listed in `docs/FILE-FORMATS.md` (#365).
//!
//! Output files follow one rule: an artifact (`--emit vm|casm|cvms|cast`) is
//! written beside the input as `<stem>.<ext>`; a view (`--emit ast`) goes to
//! stdout; `-o FILE` overrides either, and `-o -` means stdout.
//!
//! # Examples
//!
//! ```bash
//! crushc program.crush                      # program.cvm1
//! crushc program.crush -o output.cvm1
//! crushc program.crush --emit casm          # program.casm (CASM JSON)
//! crushc program.crush --emit casm -o - | jq .manifest
//! crushc program.crush --emit cvms          # program.cvms (CVM1 assembly)
//! crushc program.crush --emit cast          # program.cast.json (CAST JSON)
//! crushc program.cast.json                  # compile CAST
//! crushc program.crush --emit ast           # the program rendered as Crush source
//! crushc cast validate|pack|unpack|diff …   # CAST file tools
//! crushc program.crush --check
//! crushc program.crush -O --cap io.print --cap str.concat
//! crushc program.crush -L ./modules -L /usr/lib/crush   # import search path (CRUSH-110)
//! ```

use std::path::PathBuf;

use clap::Parser as ClapParser;

use crush_lang_sdk::MessageFormat;
use crush_lang_sdk::theme::JsonDiagnosticExt as _;

#[derive(ClapParser)]
#[command(name = "crushc")]
#[command(author, version = concat!("0.2.0"))]
#[command(about = "Compile Crush source (or CAST) to CVM1 bytecode")]
#[command(long_about = "\
crushc compiles `.crush` source files — or CAST IR (`.cast.json`, `.cast`,\n\
`.castb`) — into executable CVM1 bytecode (`.cvm1`). It can also emit\n\
CASM JSON (`.casm`), CVM1 assembly (`.cvms`) or CAST JSON (`.cast.json`).\n\
\n\
Artifacts are written beside the input as `<stem>.<ext>`; `--emit ast`\n\
prints to stdout. `-o FILE` overrides, `-o -` writes to stdout.\n\
\n\
`crushc cast validate|pack|unpack|diff FILE…` are the CAST file tools.\n\
\n\
Supports Crush frontend features: functions, if/else, while loops,\n\
variables, capability calls, string operations, and the standard library.")]
struct Cli {
    /// The input: `.crush` source, or CAST (`.cast.json`, `.cast`, `.castb`).
    input: PathBuf,

    /// Write output to FILE; `-` is stdout. Default: artifacts go to
    /// `<input stem>.<ext>` beside the input (vm → .cvm1, casm → .casm,
    /// cvms → .cvms, cast → .cast.json), `ast` goes to stdout.
    #[arg(short = 'o', long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// What to emit: vm (CVM1 bytecode, default), casm (CASM JSON), cvms
    /// (CVM1 assembly text, what crush-compile assembles), cast (CAST JSON;
    /// CBOR with `-o x.castb`), ast (the parsed program rendered back as Crush
    /// source — a view, not CAST). `types` is not implemented (#371).
    #[arg(long, value_name = "WHAT", default_value = "vm")]
    emit: EmitKind,

    /// Only check the program for errors; don't produce any output.
    #[arg(short = 'C', long)]
    check: bool,

    /// Enable the optimizer pass.
    #[arg(short = 'O', long)]
    optimize: bool,

    /// Emit `cap_call "invariant.evaluate"` instructions at the top of every
    /// function targeted by an `@invariant` block with a non-empty
    /// `check_source`. Default: off (invariant.check_source is stored, not
    /// executed, at compile-time). When enabled, the compiler also registers
    /// the `invariant.evaluate` capability permission automatically.
    #[arg(long)]
    invariant_runtime: bool,

    /// Declare a capability permission (repeatable).
    #[arg(long = "cap", value_name = "CAP")]
    caps: Vec<String>,

    /// Add a directory to the import search path (repeatable). `import a.b`
    /// looks for `a/b.crush` in the input file's directory, then each `-L`
    /// directory in order, then each `CRUSH_PATH` directory.
    #[arg(short = 'L', long = "lib-path", value_name = "DIR")]
    lib_paths: Vec<PathBuf>,

    /// Language edition year (default: 2025).
    #[arg(long = "edition", value_name = "YEAR", default_value = "2025")]
    edition: String,

    /// Print verbose compilation details to stderr.
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Fail the build (exit 1, after printing them) if the annotation checks
    /// (stale `@wip`, expired `@temporary`, wildcard in an exhaustive match,
    /// `@must-call-after`) produce any warning. Without it they are printed
    /// and the build goes on.
    #[arg(long = "deny-warnings")]
    deny_warnings: bool,

    /// Format for diagnostic output on errors: `text` (default, colored
    /// terminal output) or `json` (newline-delimited records for editor /
    /// IDE / LSP bridge integration).
    #[arg(long = "message-format", value_name = "FORMAT", default_value = "text")]
    message_format: MessageFormat,
}

#[derive(Clone, Copy, PartialEq)]
enum EmitKind {
    Vm,
    Casm,
    Cvms,
    Cast,
    Ast,
    Types,
}

impl EmitKind {
    /// The default file extension of an artifact, `None` for a view.
    fn artifact_ext(self) -> Option<&'static str> {
        match self {
            EmitKind::Vm => Some("cvm1"),
            EmitKind::Casm => Some("casm"),
            EmitKind::Cvms => Some("cvms"),
            EmitKind::Cast => Some("cast.json"),
            EmitKind::Ast | EmitKind::Types => None,
        }
    }
}

impl std::str::FromStr for EmitKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vm" => Ok(EmitKind::Vm),
            "casm" => Ok(EmitKind::Casm),
            "cvms" => Ok(EmitKind::Cvms),
            "cast" => Ok(EmitKind::Cast),
            "ast" => Ok(EmitKind::Ast),
            "types" => Ok(EmitKind::Types),
            _ => Err(format!(
                "unknown emit kind '{s}' (expected: vm, casm, cvms, cast, ast)"
            )),
        }
    }
}

fn main() {
    // `crushc cast <tool> …`: the CAST file tools (#365, ported from
    // exosphere's `ant crush cast`). A file literally named `cast` is not a
    // compilable input (it has no `.crush` / `.cast.json` extension).
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str) == Some("cast") {
        crush_lang_sdk::theme::init_styling();
        std::process::exit(crush_lang_sdk::cast_cli::main(&argv[2..]));
    }
    let cli = Cli::parse();
    crush_lang_sdk::theme::init_styling();
    if let Err(e) = run_compiler(&cli) {
        // Non-themed errors (file I/O, unknown emit kind, etc.) keep the
        // original `crushc: <msg>` prefix in default text mode. In
        // `--message-format json` mode they emit a single NDJSON record so
        // editors see a uniform stream regardless of failure origin. Themed
        // errors in `run_compiler` already print their own diagnostics
        // directly and `process::exit(1)` themselves without reaching this
        // arm.
        match cli.message_format {
            MessageFormat::Text => {
                eprintln!("crushc: {e:#}");
            }
            MessageFormat::Json => {
                let file = cli.input.display().to_string();
                eprint!(
                    "{}",
                    crush_lang_sdk::theme::render_diagnostics_ndjson(
                        &crush_lang_sdk::theme::diagnostics_for_error(
                            &e,
                            Some(&file),
                            crush_lang_sdk::theme::JsonDiagnostic::CODE_IO,
                        )
                    )
                );
            }
        }
        std::process::exit(1);
    }
}

fn run_compiler(cli: &Cli) -> anyhow::Result<()> {
    if cli.emit == EmitKind::Types {
        anyhow::bail!(
            "--emit types is not implemented yet (#371): it used to print the same \
             source render as --emit ast, with no types"
        );
    }
    use crush_lang_sdk::formats::{self, InputFormat};
    let format = formats::detect_or_bail(&cli.input, matches!(cli.message_format, MessageFormat::Text))?;
    if !matches!(format, InputFormat::Crush) && !format.is_cast() {
        anyhow::bail!(
            "crushc compiles `.crush` source or CAST (`.cast.json`, `.cast`, `.castb`); \
             '{}' is already compiled — run it with `crush-run run`",
            cli.input.display()
        );
    }

    if cli.verbose {
        eprintln!("crushc: reading '{}' ({format:?})", cli.input.display());
        eprintln!("crushc: edition {}", cli.edition);
        if !cli.lib_paths.is_empty() {
            eprintln!(
                "crushc: library paths: {}",
                cli.lib_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    // ── Step 1: Parse to CAST (Crush Abstract Syntax Tree) ──────────
    // A CAST input is loaded through the version-gated `Program::load` path.
    let source = if format.is_cast() {
        String::new()
    } else {
        std::fs::read_to_string(&cli.input)
            .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", cli.input.display()))?
    };
    let parsed = if format.is_cast() {
        Ok(formats::load_cast(&cli.input, format)?)
    } else {
        crush_frontend::parser::Parser::parse(&source)
    };
    let mut program = match parsed {
        Ok(p) => p,
        Err(errors) => {
            let file = cli.input.display().to_string();
            // Themed output goes to stderr directly; we exit immediately so
            // the user never sees a duplicated `crushc: parse failed`
            // message on top of the pretty diagnostic.
            match cli.message_format {
                MessageFormat::Text => {
                    eprint!(
                        "{}",
                        crush_lang_sdk::theme::render_parse_errors(&errors, Some(&file), &source)
                    );
                }
                MessageFormat::Json => {
                    let diags: Vec<crush_lang_sdk::theme::JsonDiagnostic> = errors
                        .iter()
                        .map(|e| {
                            crush_lang_sdk::theme::JsonDiagnostic::parse_error(
                                e,
                                Some(&file),
                            )
                        })
                        .collect();
                    eprint!(
                        "{}",
                        crush_lang_sdk::theme::render_diagnostics_ndjson(&diags)
                    );
                }
            }
            std::process::exit(1);
        }
    };

    // `--emit ast` dumps the file as written; everything else compiles the
    // program with its imports resolved and merged (CRUSH-110).
    if cli.emit != EmitKind::Ast {
        let imports =
            crush_frontend::import_system::ImportOptions::with_crush_path(cli.lib_paths.clone());
        if let Err(e) =
            crush_frontend::import_system::resolve_imports(&mut program, &cli.input, &imports)
        {
            report_import_error(cli, &e);
        }
    }

    // CRUSH-369: the annotation checks `crush-lint` runs (stale @wip, expired
    // @temporary, exhaustiveness, @must-call-after) as warnings. Run on the
    // enriched, import-merged program; never fatal unless --deny-warnings.
    if cli.emit != EmitKind::Ast {
        let mut checked = program.clone();
        crush_frontend::cast_enrich::enrich_cast(&mut checked);
        let warnings =
            crush_lang_sdk::compile::annotation_diagnostics(&checked, &cli.input.display().to_string());
        crush_lang_sdk::compile::print_annotation_diagnostics(
            &warnings,
            matches!(cli.message_format, MessageFormat::Json),
        );
        if cli.deny_warnings && !warnings.is_empty() {
            if matches!(cli.message_format, MessageFormat::Text) {
                eprintln!(
                    "crushc: {} warning(s) treated as errors (--deny-warnings)",
                    warnings.len()
                );
            }
            std::process::exit(1);
        }
    }

    // `--emit cast`: the program as parsed, imports merged, before the
    // polyglot pass (which the loader runs again when the CAST is compiled).
    if cli.emit == EmitKind::Cast {
        program.cast_version = crush_cast::CAST_VERSION.to_string();
        let dest = destination(cli);
        let codec = match &dest {
            Dest::File(p) => crush_cast::Format::from_path(p),
            Dest::Stdout => crush_cast::Format::Json,
        };
        let bytes = program.serialize(codec).map_err(|e| anyhow::anyhow!("{e}"))?;
        return write_artifact(cli, &dest, &bytes, "");
    }

    // Same pass `compile_crush_source` (the `crush-run x.crush` path) runs:
    // without it `@lang` blocks get no variable marshaling (CRUSH-130).
    // It also collects the CRUSH-270 polyglot warnings (implicit output,
    // clobber), surfaced to stderr below.
    let warnings = crush_lang_sdk::compile::prepare_polyglot_blocks(&mut program)?;
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    if cli.emit == EmitKind::Ast {
        let rendered = crush_frontend::render::render_program(&program);
        let dest = destination(cli);
        return write_artifact(cli, &dest, rendered.as_bytes(), "");
    }

    if cli.verbose {
        eprintln!("crushc: parse OK ({} functions)", program.functions.len());
    }

    // ── Step 2: Semantic analysis + type-checking ───────────────────
    // Lowered first, as every library compile path does, so the
    // constructors of constructed Kinds are declared (#260).
    let (mut lowered, cosm) = match crush_frontend::lower_and_check(program.clone()) {
        Ok(checked) => checked,
        Err(e) => {
            // Render the underlying message under a `[type]` badge so the
            // interface still feels consistent with parse errors. Exit
            // immediately so we don't double-print at the main() error arm.
            let body = e.to_string();
            let file = cli.input.display().to_string();
            match cli.message_format {
                MessageFormat::Text => {
                    eprint!(
                        "{badge} type error: {body}\n",
                        badge = crush_lang_sdk::theme::paint_error_badge("type"),
                        body = body,
                    );
                }
                MessageFormat::Json => {
                    eprint!(
                        "{}",
                        crush_lang_sdk::theme::render_diagnostics_ndjson(
                            &crush_lang_sdk::theme::diagnostics_for_error(
                                &e,
                                Some(&file),
                                crush_lang_sdk::theme::JsonDiagnostic::CODE_TYPE,
                            )
                        )
                    );
                }
            }
            std::process::exit(1);
        }
    };

    if cli.verbose {
        eprintln!("crushc: type-check OK");
    }

    // --check: stop after type-checking
    if cli.check {
        eprintln!("crushc: no errors detected");
        return Ok(());
    }

    // ── Step 3: Optimization (optional) ─────────────────────────────
    if cli.optimize {
        if cli.verbose {
            eprintln!("crushc: optimizing...");
        }
        crush_frontend::optimizer::Optimizer::optimize(&mut lowered);
        if cli.verbose {
            let fn_count = lowered.functions.len();
            eprintln!("crushc: optimization done ({fn_count} functions)");
        }
    }

    // ── Step 4: Compile to CASM then VM ─────────────────────────────
    if cli.verbose && cli.invariant_runtime {
        eprintln!("crushc: invariant runtime checks enabled");
    }
    let mut compiler =
        crush_frontend::compiler::Compiler::new().with_invariant_runtime(cli.invariant_runtime);
    let casm_prog = compiler.compile_lowered(lowered, cosm)?;
    let dest = destination(cli);
    match cli.emit {
        EmitKind::Casm => {
            // CASM JSON, the `casm::Program` IR (`.casmb` → MessagePack).
            let codec = match &dest {
                Dest::File(p) => casm::Format::from_path(p),
                Dest::Stdout => casm::Format::Json,
            };
            if codec == casm::Format::Binary && !casm_prog.functions.is_empty() {
                anyhow::bail!(
                    "binary CASM (.casmb) can't hold a program with functions yet — it would \
                     not load back (#372); write CASM JSON (.casm) instead"
                );
            }
            let bytes = casm_prog.serialize(codec).map_err(|e| anyhow::anyhow!("{e}"))?;
            write_artifact(cli, &dest, &bytes, "")
        }
        EmitKind::Cvms => {
            // CVM1 assembly: the bytecode disassembled, with its `.cap`
            // header, in the form `crush-compile` / `crush_vm::assemble` read.
            let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm_prog)?;
            let mut text = crush_vm::disassemble(&vm_prog);
            if !text.ends_with('\n') {
                text.push('\n');
            }
            write_artifact(cli, &dest, text.as_bytes(), "")
        }
        EmitKind::Vm => {
            let vm_program = crush_lang_sdk::compile::casm_to_vm(&casm_prog)?;
            if cli.verbose {
                eprintln!(
                    "crushc: compiled OK ({} instructions, {} constants)",
                    vm_program.code.len(),
                    vm_program.consts.len(),
                );
            }
            let blob = vm_program.to_blob();
            let detail = format!("{} instructions, ", vm_program.code.len());
            write_artifact(cli, &dest, &blob, &detail)
        }
        _ => unreachable!(),
    }
}

/// Where an emit goes: stdout or a file.
enum Dest {
    Stdout,
    File(PathBuf),
}

/// `-o -` → stdout, `-o FILE` → FILE; otherwise an artifact goes beside the
/// input as `<stem>.<ext>` and a view to stdout.
fn destination(cli: &Cli) -> Dest {
    match &cli.output {
        Some(p) if p.as_os_str() == "-" => Dest::Stdout,
        Some(p) => Dest::File(p.clone()),
        None => match cli.emit.artifact_ext() {
            Some(ext) => Dest::File(crush_lang_sdk::formats::artifact_path(&cli.input, ext)),
            None => Dest::Stdout,
        },
    }
}

/// Write `bytes` to `dest`; a file write reports the path and real size.
fn write_artifact(cli: &Cli, dest: &Dest, bytes: &[u8], detail: &str) -> anyhow::Result<()> {
    match dest {
        Dest::Stdout => {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            out.write_all(bytes)?;
            out.flush()?;
        }
        Dest::File(path) => {
            std::fs::write(path, bytes)
                .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", path.display()))?;
            println!(
                "  Compiled {} → {} ({detail}{} bytes)",
                cli.input.display(),
                path.display(),
                bytes.len(),
            );
        }
    }
    Ok(())
}

/// Print an import-resolution failure (text: `[import] error: file:line:col:
/// …`; json: an `E-IM01` record in the file the problem is in) and exit 1.
fn report_import_error(cli: &Cli, e: &anyhow::Error) -> ! {
    match cli.message_format {
        MessageFormat::Text => {
            eprintln!(
                "{badge} error: {e}",
                badge = crush_lang_sdk::theme::paint_error_badge("import"),
            );
        }
        MessageFormat::Json => {
            let file = cli.input.display().to_string();
            eprint!(
                "{}",
                crush_lang_sdk::theme::render_diagnostics_ndjson(
                    &crush_lang_sdk::theme::diagnostics_for_error(
                        e,
                        Some(&file),
                        crush_lang_sdk::theme::JsonDiagnostic::CODE_IMPORT,
                    )
                )
            );
        }
    }
    std::process::exit(1);
}
