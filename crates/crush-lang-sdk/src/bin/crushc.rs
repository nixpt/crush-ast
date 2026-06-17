//! `crushc` — the Crush compiler, akin to `rustc`.
//!
//! Compiles `.crush` source files to CVM1 bytecode (`.cvm1`), with optional
//! intermediate-representation emission (AST, CASM, type-annotated AST).
//!
//! # Examples
//!
//! ```bash
//! crushc program.crush
//! crushc program.crush -o output.cvm1
//! crushc program.crush --emit casm
//! crushc program.crush --emit ast
//! crushc program.crush --check
//! crushc program.crush -O --cap io.print --cap str.concat
//! crushc program.crush -L ./modules -L /usr/lib/crush
//! ```

use std::fmt::Write;
use std::path::PathBuf;

use clap::Parser as ClapParser;

#[derive(ClapParser)]
#[command(name = "crushc")]
#[command(author, version = concat!("0.2.0"))]
#[command(about = "Compile Crush source files to CVM1 bytecode")]
#[command(long_about = "\
crushc compiles `.crush` source files into executable CVM1 bytecode.\n\
It can also emit intermediate representations for debugging.\n\
\n\
Supports Crush frontend features: functions, if/else, while loops,\n\
variables, capability calls, string operations, and the standard library.")]
struct Cli {
    /// Path to the input `.crush` source file.
    input: PathBuf,

    /// Write output to FILE (default: <input_stem>.cvm1).
    #[arg(short = 'o', long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// What to emit: vm (default), casm (assembly text), ast (CAST dump),
    /// types (type-annotated AST).
    #[arg(long, value_name = "WHAT", default_value = "vm")]
    emit: EmitKind,

    /// Only check the program for errors; don't produce any output.
    #[arg(short = 'C', long)]
    check: bool,

    /// Enable the optimizer pass.
    #[arg(short = 'O', long)]
    optimize: bool,

    /// Declare a capability permission (repeatable).
    #[arg(long = "cap", value_name = "CAP")]
    caps: Vec<String>,

    /// Add a directory to the library search path (for import resolution).
    #[arg(short = 'L', long = "lib-path", value_name = "DIR")]
    lib_paths: Vec<PathBuf>,

    /// Language edition year (default: 2025).
    #[arg(long = "edition", value_name = "YEAR", default_value = "2025")]
    edition: String,

    /// Print verbose compilation details to stderr.
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum EmitKind {
    Vm,
    Casm,
    Ast,
    Types,
}

impl std::str::FromStr for EmitKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vm" => Ok(EmitKind::Vm),
            "casm" => Ok(EmitKind::Casm),
            "ast" => Ok(EmitKind::Ast),
            "types" => Ok(EmitKind::Types),
            _ => Err(format!(
                "unknown emit kind '{s}' (expected: vm, casm, ast, types)"
            )),
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run_compiler(&cli) {
        eprintln!("crushc: {e:#}");
        std::process::exit(1);
    }
}

fn run_compiler(cli: &Cli) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(&cli.input)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", cli.input.display()))?;

    if cli.verbose {
        eprintln!(
            "crushc: reading '{}' ({} bytes)",
            cli.input.display(),
            source.len()
        );
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
    let mut program = crush_frontend::parser::Parser::parse(&source).map_err(|errors| {
        let mut msg = String::from("parse error");
        for (i, err) in errors.iter().enumerate() {
            use std::fmt::Write;
            let _ = write!(msg, "\n  {}. {err}", i + 1);
        }
        anyhow::anyhow!(msg)
    })?;

    if cli.emit == EmitKind::Ast {
        let rendered = crush_frontend::render::render_program(&program);
        emit_text_output(cli, &rendered)?;
        return Ok(());
    }

    if cli.verbose {
        eprintln!("crushc: parse OK ({} functions)", program.functions.len());
    }

    // ── Step 2: Semantic analysis + type-checking ───────────────────
    let mut semantics = crush_frontend::semantics::SemanticAnalyzer::new();
    semantics
        .check(&program)
        .map_err(|e| anyhow::anyhow!("type error: {e}"))?;

    if cli.emit == EmitKind::Types {
        let rendered = crush_frontend::render::render_program(&program);
        emit_text_output(cli, &rendered)?;
        return Ok(());
    }

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
        crush_frontend::optimizer::Optimizer::optimize(&mut program);
        if cli.verbose {
            let fn_count = program.functions.len();
            eprintln!("crushc: optimization done ({fn_count} functions)");
        }
    }

    // ── Step 4: Compile to CASM then VM ─────────────────────────────
    match cli.emit {
        EmitKind::Casm => {
            let mut compiler = crush_frontend::compiler::Compiler::new();
            let casm_prog = compiler.compile(program)?;
            let casm_text = format_casm_program(&casm_prog, &cli.caps);
            emit_text_output(cli, &casm_text)?;
            Ok(())
        }
        EmitKind::Vm => {
            let mut compiler = crush_frontend::compiler::Compiler::new();
            let casm_prog = compiler.compile(program)?;
            let vm_program = crush_lang_sdk::compile::casm_to_vm(&casm_prog)?;

            if cli.verbose {
                eprintln!(
                    "crushc: compiled OK ({} instructions, {} constants)",
                    vm_program.code.len(),
                    vm_program.consts.len(),
                );
            }

            let out_path = cli
                .output
                .clone()
                .unwrap_or_else(|| cli.input.with_extension("cvm1"));

            if cli.verbose {
                eprintln!(
                    "crushc: writing {} bytes to '{}'",
                    vm_program.code.len(),
                    out_path.display()
                );
            }

            let blob = vm_program.to_blob();
            std::fs::write(&out_path, &blob)
                .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", out_path.display()))?;

            println!(
                "  Compiled {} → {} ({} instructions, {} bytes)",
                cli.input.display(),
                out_path.display(),
                vm_program.code.len(),
                blob.len(),
            );
            Ok(())
        }
        _ => unreachable!(),
    }
}

/// Emit text to stdout (no -o) or to a file.
fn emit_text_output(cli: &Cli, content: &str) -> anyhow::Result<()> {
    if let Some(ref path) = cli.output {
        std::fs::write(path, content)
            .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", path.display()))?;
        eprintln!(
            "crushc: wrote '{}' ({} bytes)",
            path.display(),
            content.len()
        );
    } else {
        print!("{content}");
    }
    Ok(())
}

/// Render a CASM program as human-readable assembly text.
fn format_casm_program(program: &casm::Program, extra_caps: &[String]) -> String {
    let mut out = String::new();

    // Permission declarations
    let all_perms: Vec<&str> = {
        let mut p: Vec<&str> = program
            .manifest
            .permissions
            .iter()
            .map(|s| s.as_str())
            .collect();
        for cap in extra_caps {
            if !p.contains(&cap.as_str()) {
                p.push(cap);
            }
        }
        p
    };
    if !all_perms.is_empty() {
        for perm in &all_perms {
            let _ = writeln!(out, ".permission {perm}");
        }
        let _ = writeln!(out);
    }

    // Function bodies
    for (fname, func) in &program.functions {
        let _ = writeln!(out, ".func {fname}");
        for instr in &func.body {
            let args_str = fmt_casm_args(&instr.args);
            let _ = writeln!(out, "    {} {}", instr.op, args_str);
        }
        let _ = writeln!(out);
    }

    out
}

fn fmt_casm_args(args: &serde_json::Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            match v {
                serde_json::Value::String(s) => {
                    let esc = s
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n")
                        .replace('\r', "\\r");
                    parts.push(format!("{k}={esc:?}"));
                }
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        parts.push(format!("{k}={i}"));
                    } else if let Some(f) = n.as_f64() {
                        parts.push(format!("{k}={f}"));
                    }
                }
                serde_json::Value::Bool(b) => parts.push(format!("{k}={b}")),
                serde_json::Value::Null => parts.push(format!("{k}=null")),
                _ => {}
            }
        }
    }
    parts.join(" ")
}
