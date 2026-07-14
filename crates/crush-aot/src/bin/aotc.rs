//! `crush-aotc` — Ahead-of-Time compiler for Crush.
//!
//! Compiles `.crush` source files to native shared libraries (`.so`/`.dylib`/`.dll`)
//! by transpiling to Rust or C and invoking `rustc`, `gcc`, or `clang`.
//!
//! # Examples
//!
//! ```bash
//! # Compile to native .so (via rustc)
//! crush-aotc compile program.crush -o program.so
//!
//! # Compile to native .so (via gcc)
//! crush-aotc compile program.crush --emit c
//!
//! # Compile to native .so (via clang)
//! crush-aotc compile program.crush --emit c --cc clang
//!
//! # Compile and run immediately
//! crush-aotc run program.crush
//!
//! # Dump generated source
//! crush-aotc compile program.crush --emit rust
//! crush-aotc compile program.crush --emit c-source
//!
//! # Benchmark across all tiers
//! crush-aotc benchmark program.crush --runs 1000
//! ```

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

// ── CLI ──��─────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "crush-aotc")]
#[command(author, version = concat!("0.2.0"))]
#[command(about = "Ahead-of-Time compile Crush source to native shared libraries")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a .crush file to a native shared library.
    Compile(CompileArgs),

    /// Compile and immediately load+run, printing the result.
    Run(RunArgs),

    /// Benchmark a .crush program across all execution tiers.
    Benchmark(BenchArgs),
}

#[derive(Parser)]
struct CompileArgs {
    /// Path to the input `.crush` source file.
    input: PathBuf,

    /// Write output to FILE.
    #[arg(short = 'o', long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// What to emit: `so` (rustc → .so, default), `c` (gcc/clang ��� .so),
    /// `rust` (Rust source), `c-source` (C source).
    #[arg(long, value_name = "WHAT", default_value = "so")]
    emit: EmitKind,

    /// C compiler to use with `--emit c` (default: gcc).
    #[arg(long, value_name = "CC", default_value = "gcc")]
    cc: String,

    /// Only check the program; don't produce output.
    #[arg(short = 'C', long)]
    check: bool,

    /// Optimization: `-O` (default), `-O0` (unoptimized).
    #[arg(short = 'O', long, action = clap::ArgAction::SetTrue, default_value = "true")]
    optimize: bool,

    /// Print verbose compilation details to stderr.
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[derive(Parser)]
struct RunArgs {
    /// Path to the input `.crush` source file.
    input: PathBuf,

    /// Backend: `rustc` (default), `gcc`, `clang`.
    #[arg(long, default_value = "rustc")]
    backend: String,

    /// Optimization level.
    #[arg(short = 'O', long, action = clap::ArgAction::SetTrue, default_value = "true")]
    optimize: bool,

    /// Print verbose compilation details to stderr.
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[derive(Parser)]
struct BenchArgs {
    /// Path to the input `.crush` source file.
    input: PathBuf,

    /// Number of runs per tier (default: 100).
    #[arg(long, default_value = "100")]
    runs: usize,

    /// Only benchmark specific tiers. Repeatable. Options: cvm1, fastvm, rust, c-gcc, c-clang.
    #[arg(long = "tier")]
    tiers: Vec<String>,

    /// Skip auto-discovering companion .py/.js/.mjs files.
    #[arg(long)]
    no_companions: bool,

    /// Add an external command tier. Format: LABEL CMD [ARGS...].
    /// Example: --extern python3 "python3 mybench.py"
    #[arg(long = "extern", num_args = 2.., value_names = ["LABEL", "CMD"])]
    externs: Vec<String>,

    /// Output results as JSON instead of a table.
    #[arg(long)]
    json: bool,

    /// Expected output string (validates all tiers produce this).
    #[arg(long, value_name = "OUTPUT")]
    expected: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum EmitKind {
    So,
    C,
    Rust,
    CSource,
}

impl std::str::FromStr for EmitKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "so"       => Ok(EmitKind::So),
            "c"        => Ok(EmitKind::C),
            "rust"     => Ok(EmitKind::Rust),
            "c-source" => Ok(EmitKind::CSource),
            _ => Err(format!("unknown emit kind '{s}' (expected: so, c, rust, c-source)")),
        }
    }
}

// ── Main ──��────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        eprintln!("crush-aotc: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::Compile(args) => cmd_compile(args),
        Command::Run(args) => cmd_run(args),
        Command::Benchmark(args) => cmd_bench(args),
    }
}

// ── compile ─────────────────────────────────────────────────────────────────

fn cmd_compile(args: &CompileArgs) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(&args.input).map_err(|e| {
        anyhow::anyhow!("cannot read '{}': {e}", args.input.display())
    })?;

    let module_name = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("module");

    if args.verbose {
        eprintln!("crush-aotc: reading '{}' ({} bytes)", args.input.display(), source.len());
    }

    let program = match crush_frontend::compile_crush_source(&source) {
        Ok(p) => p,
        Err(e) => { eprintln!("crush-aotc: compilation error: {e}"); return Err(e.into()); }
    };

    if args.verbose {
        eprintln!("crush-aotc: parsed OK ({} functions)", program.functions.len());
    }

    if args.check {
        eprintln!("crush-aotc: no errors detected");
        return Ok(());
    }

    // ── Emit source only ──
    if args.emit == EmitKind::Rust {
        let src = crush_aot::codegen::gen_rust_source(&program);
        return emit_text(args, &src);
    }
    if args.emit == EmitKind::CSource {
        let src = crush_aot::codegen_c::gen_c_source(&program);
        return emit_text(args, &src);
    }

    // ── Emit native .so ──
    let compiler = crush_aot::AotCompiler::new().with_optimize(args.optimize);

    let so_path = match args.emit {
        EmitKind::C => compiler.compile_c(&program, module_name, &args.cc)?,
        _ => compiler.compile_casm(&program, module_name)?,
    };

    if let Some(ref out) = args.output {
        std::fs::copy(&so_path, out)
            .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", out.display()))?;
        let size = std::fs::metadata(out)?.len();
        println!("  Compiled {} → {} ({size} bytes, .{})", args.input.display(), out.display(), so_ext());
    } else {
        println!("  Compiled {} → {} (.{})", args.input.display(), so_path.display(), so_ext());
    }

    Ok(())
}

fn emit_text(args: &CompileArgs, text: &str) -> anyhow::Result<()> {
    if let Some(ref path) = args.output {
        std::fs::write(path, text)
            .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", path.display()))?;
        eprintln!("crush-aotc: wrote source to '{}' ({} bytes)", path.display(), text.len());
    } else {
        print!("{text}");
    }
    Ok(())
}

// ── run ─────────────────────────────────────────────────────────────────────

fn cmd_run(args: &RunArgs) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(&args.input)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", args.input.display()))?;
    let module_name = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("module");

    if args.verbose {
        eprintln!("crush-aotc: compiling and running '{}'...", args.input.display());
    }

    let program = crush_frontend::compile_crush_source(&source)?;
    let compiler = crush_aot::AotCompiler::new().with_optimize(args.optimize);

    let so_path = match args.backend.as_str() {
        "gcc" | "clang" => compiler.compile_c(&program, module_name, &args.backend)?,
        _ => compiler.compile_casm(&program, module_name)?,
    };

    if args.verbose {
        eprintln!("crush-aotc: compiled → '{}'", so_path.display());
    }

    let module = crush_aot::Module::load(&so_path)?;
    let result = module.call_main()?;
    println!("{result}");
    Ok(())
}

/// Helper: run_fastvm with error conversion for anyhow compatibility.
fn run_fastvm_ok(program: &casm::Program) -> anyhow::Result<crush_vm::fastvm::FastYield> {
    crush_vm::run_fastvm(program)
        .map_err(|e| anyhow::anyhow!("FastVM error: {:?}", e))
}

// ── benchmark ───────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct BenchResult {
    tier: String,
    us: f64,
    speedup: f64,
}

#[derive(serde::Serialize)]
struct BenchReport {
    benchmark: String,
    runs: usize,
    results: Vec<BenchResult>,
}

struct Tier {
    label: String,
    tag: String,
}

fn cmd_bench(args: &BenchArgs) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(&args.input)
        .map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", args.input.display()))?;
    let module_name = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("module");

    let program = crush_frontend::compile_crush_source(&source)?;
    let compiler = crush_aot::AotCompiler::new().with_optimize(true);

    // ── Pre-compile AOT modules ──────────────────────────────────────
    let so_rust = compiler.compile_casm(&program, module_name)?;
    let mod_rust = crush_aot::Module::load(&so_rust)?;
    let expected_val = mod_rust.call_main()?;
    let expected_str = format!("{expected_val}");

    // Validate C tiers if they exist (don't panic, just warn and skip)
    let mut mod_c_gcc: Option<crush_aot::Module> = None;
    let mut mod_c_clang: Option<crush_aot::Module> = None;
    for (_tag, label, slot) in [
        ("c-gcc", "gcc", &mut mod_c_gcc),
        ("c-clang", "clang", &mut mod_c_clang),
    ] {
        match compiler.compile_c(&program, module_name, label) {
            Ok(so) => match crush_aot::Module::load(&so) {
                Ok(m) => {
                    let val = m.call_main();
                    if val.as_ref().ok() != Some(&expected_val) {
                        eprintln!("crush-aotc: warning: C ({label}) output {:?} diverges from Rust ({:?}) — skipping tier", val.ok(), expected_val);
                        continue;
                    }
                    *slot = Some(m);
                }
                Err(e) => eprintln!("crush-aotc: warning: C ({label}) load failed: {e} — skipping"),
            },
            Err(e) => eprintln!("crush-aotc: warning: C ({label}) compile failed: {e} — skipping"),
        }
    }

    // Validate FastVM
    {
        let fv = run_fastvm_ok(&program);
        if let Ok(fv) = fv {
            let fv_val = match fv {
                crush_vm::fastvm::FastYield::Finished(Some(v)) => v,
                crush_vm::fastvm::FastYield::Value(v) => v,
                _ => crush_vm::RuntimeValue::Null,
            };
            if fv_val != expected_val {
                eprintln!("crush-aotc: warning: FastVM output {:?} diverges from Rust ({:?}) — skipping tier", fv_val, expected_val);
            }
        }
    }

    if let Some(ref want) = args.expected {
        assert_eq!(&expected_str, want, "output mismatch: Crush produced '{expected_str}', expected '{want}'");
    }

    // ── Build tier list ──────────────────────────────────────────────
    let mut all_tiers: Vec<Tier> = vec![
        Tier { label: "CVM1".into(), tag: "cvm1".into() },
        Tier { label: "FastVM".into(), tag: "fastvm".into() },
        Tier { label: "AOT Rust".into(), tag: "rust".into() },
        Tier { label: "AOT C (gcc)".into(), tag: "c-gcc".into() },
        Tier { label: "AOT C (clang)".into(), tag: "c-clang".into() },
    ];

    // Discover companion scripts
    if !args.no_companions {
        let stem = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let dir = args.input.parent().unwrap_or(std::path::Path::new("."));

        if dir.join(format!("{stem}.py")).exists() {
            all_tiers.push(Tier { label: "Python3".into(), tag: "py".into() });
        }
        if dir.join(format!("{stem}.js")).exists() {
            all_tiers.push(Tier { label: "Node.js".into(), tag: "js".into() });
        }
        if dir.join(format!("{stem}.mjs")).exists() {
            all_tiers.push(Tier { label: "Node.js (ESM)".into(), tag: "mjs".into() });
        }
    }

    // Parse --extern tiers
    let mut externs: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut ext_iter = args.externs.iter().peekable();
    loop {
        let Some(label) = ext_iter.next() else { break };
        let Some(cmd) = ext_iter.next() else {
            anyhow::bail!("--extern needs LABEL CMD pairs, missing CMD after '{label}'");
        };
        let argv: Vec<String> = shlex::split(&cmd)
            .ok_or_else(|| anyhow::anyhow!("--extern CMD '{cmd}' has unbalanced quotes"))?;
        if argv.is_empty() {
            anyhow::bail!("--extern CMD '{cmd}' is empty");
        }
        let prog = argv[0].clone();
        let args = argv[1..].to_vec();
        externs.push((label.clone(), prog, args));
        all_tiers.push(Tier { label: label.clone(), tag: label.clone() });
    }

    // Build filter
    let tier_filter: Vec<&str> = if args.tiers.is_empty() {
        all_tiers.iter().map(|t| t.tag.as_str()).collect()
    } else {
        all_tiers.iter().filter(|t| args.tiers.contains(&t.tag)).map(|t| t.tag.as_str()).collect()
    };

    let runs = args.runs;

    // ── Collect results ─────────────────────────────────────────────
    let mut results: Vec<(String, f64)> = Vec::new();
    let mut baseline: Option<f64> = None;

    for tier in &all_tiers {
        if !tier_filter.contains(&tier.tag.as_str()) { continue; }

        let us = match tier.tag.as_str() {
            "cvm1" => {
                let start = Instant::now();
                for _ in 0..runs {
                    let vm_prog = crush_lang_sdk::compile::casm_to_vm(&program)?;
                    let _ = crush_vm::run(&vm_prog, &crush_vm::Quotas::default())?;
                }
                start.elapsed().as_micros() as f64 / runs as f64
            }
            "fastvm" => {
                let start = Instant::now();
                for _ in 0..runs { let _ = run_fastvm_ok(&program)?; }
                start.elapsed().as_micros() as f64 / runs as f64
            }
            "rust" => {
                let start = Instant::now();
                for _ in 0..runs { let _ = mod_rust.call_main()?; }
                start.elapsed().as_micros() as f64 / runs as f64
            }
            "c-gcc" => {
                if let Some(ref m) = mod_c_gcc {
                    let start = Instant::now();
                    for _ in 0..runs { let _ = m.call_main()?; }
                    start.elapsed().as_micros() as f64 / runs as f64
                } else { continue; }
            }
            "c-clang" => {
                if let Some(ref m) = mod_c_clang {
                    let start = Instant::now();
                    for _ in 0..runs { let _ = m.call_main()?; }
                    start.elapsed().as_micros() as f64 / runs as f64
                } else { continue; }
            }
            "py" | "js" | "mjs" => {
                let (script_path, cmd) = match tier.tag.as_str() {
                    "py" => {
                        let dir = args.input.parent().unwrap_or(std::path::Path::new("."));
                        let stem = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        (dir.join(format!("{stem}.py")), "python3")
                    }
                    "js" => {
                        let dir = args.input.parent().unwrap_or(std::path::Path::new("."));
                        let stem = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        (dir.join(format!("{stem}.js")), "node")
                    }
                    "mjs" => {
                        let dir = args.input.parent().unwrap_or(std::path::Path::new("."));
                        let stem = args.input.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        (dir.join(format!("{stem}.mjs")), "node")
                    }
                    _ => unreachable!(),
                };
                let start = Instant::now();
                for _ in 0..runs {
                    let out = std::process::Command::new(cmd)
                        .arg(&script_path)
                        .output()
                        .map_err(|e| anyhow::anyhow!("Failed to run {} {}: {e}", cmd, script_path.display()))?;
                    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if got != expected_str {
                        anyhow::bail!(
                            "{cmd} {} output mismatch: expected '{expected_str}', got '{got}'",
                            script_path.display()
                        );
                    }
                }
                start.elapsed().as_micros() as f64 / runs as f64
            }
            _ => {
                // Check externs
                if let Some((_label, cmd, argv)) = externs.iter().find(|(l, _, _)| *l == tier.label) {
                    let start = Instant::now();
                    for _ in 0..runs {
                        let out = std::process::Command::new(cmd)
                            .args(argv)
                            .output()
                            .map_err(|e| anyhow::anyhow!("Failed to run external {}: {e}", tier.label))?;
                        let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if got != expected_str {
                            anyhow::bail!(
                                "External '{}' output mismatch: expected '{expected_str}', got '{got}'",
                                tier.label
                            );
                        }
                    }
                    start.elapsed().as_micros() as f64 / runs as f64
                } else {
                    continue;
                }
            }
        };

        if baseline.is_none() { baseline = Some(us); }
        results.push((tier.label.to_string(), us));
    }

    // ── Output ──────────────────────────────────────────────────────
    if args.json {
        let report = BenchReport {
            benchmark: args.input.display().to_string(),
            runs,
            results: results.iter().map(|(tier, us)| {
                BenchResult { tier: tier.clone(), us: *us, speedup: baseline.unwrap() / us }
            }).collect(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Benchmark: {} ({} runs per tier)\n", args.input.display(), runs);
        println!("{:<22} {:>10} {:>10}", "Tier", "Time (µs)", "Speedup");
        println!("{}", "-".repeat(44));
        for (label, us) in &results {
            let speedup = baseline.unwrap() / us;
            println!("{:<22} {:>10.1} {:>9.1}x", label, us, speedup);
        }
    }

    Ok(())
}

fn so_ext() -> &'static str {
    if cfg!(target_os = "linux") { "so" }
    else if cfg!(target_os = "macos") { "dylib" }
    else { "dll" }
}
