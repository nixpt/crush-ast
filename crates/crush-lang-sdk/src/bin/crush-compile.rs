//! `crush-compile` — assemble CVM1 assembly (`.cvms`) to CVM1 bytecode
//! (`.cvm1`). Also takes CASM JSON (`.casm`, `.sno`) and, for one release, a
//! text `.casm` (with a note to rename it). Formats: `docs/FILE-FORMATS.md`.
//!
//! # Examples
//!
//! ```bash
//! crush-compile hello.cvms                  # hello.cvm1 beside it
//! crush-compile hello.cvms -o hello.cvm1 --cap fs.read
//! crush-compile hello.cvms --name hello_world
//! crush-compile hello.casm                  # CASM JSON → CVM1
//! ```
//!
//! The `.cap` lines in the text declare its capabilities (`crushc --emit
//! cvms` writes them); `--cap` adds more.

use std::path::PathBuf;

use clap::Parser;

use crush_lang_sdk::MessageFormat;

#[derive(Parser)]
#[command(name = "crush-compile")]
#[command(about = "Assemble CVM1 assembly (.cvms) or CASM JSON (.casm) to CVM1 bytecode (.cvm1)")]
struct Cli {
    /// Input: CVM1 assembly (`.cvms`) or CASM JSON (`.casm`, `.sno`).
    input: PathBuf,

    /// Output CVM1 file (default: `<input stem>.cvm1` beside the input; `-` is stdout).
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Program name stored in the manifest.
    #[arg(short, long, value_name = "NAME")]
    name: Option<String>,

    /// Grant a capability permission (repeatable).
    #[arg(long = "cap", value_name = "CAP")]
    caps: Vec<String>,

    /// Format for diagnostic output on errors: `text` (default,
    /// `crush-compile: <msg>` prefix) or `json` (NDJSON records for
    /// editor / IDE / LSP bridge integration). Mirrors `crushc` and
    /// `crush-run`.
    #[arg(long = "message-format", value_name = "FORMAT", default_value = "text")]
    message_format: MessageFormat,
}

fn main() {
    let cli = Cli::parse();
    crush_lang_sdk::theme::init_styling();
    let notes = matches!(cli.message_format, MessageFormat::Text);
    if let Err(e) = compile(&cli.input, cli.output, cli.name, cli.caps, notes) {
        // Mirrors the wiring in `crushc` and `crush-run`. The CLI
        // fallback path calls `crush_lang_sdk::assemble` directly (not
        // `Runtime::run_casm`), so we downcast on the assembler's typed
        // error to route CASM-syntax failures to the dedicated `"E-ASM"`
        // code with the input file attached. Everything else flows through
        // `JsonDiagnostic::generic_error` with `CODE_IO` (file open / read
        // / write failures). Text mode keeps the original `crush-compile:`
        // prefix so default UX is unchanged.
        let file_str = cli.input.display().to_string();
        match cli.message_format {
            MessageFormat::Text => {
                eprintln!("crush-compile: {e:#}");
            }
            MessageFormat::Json => {
                let diag = if let Some(asm_err) =
                    e.downcast_ref::<crush_vm::AssemblyError>()
                {
                    // `AssemblyError`'s `Display` already includes the line
                    // number (e.g. `line 3: duplicate label \"foo\"`), so
                    // passing it through carries the source position into
                    // the JSON record alongside the `file` field.
                    crush_lang_sdk::theme::JsonDiagnostic::assembler_error(
                        &asm_err.to_string(),
                        Some(&file_str),
                    )
                } else {
                    crush_lang_sdk::theme::JsonDiagnostic::generic_error(
                        &e.to_string(),
                        crush_lang_sdk::theme::JsonDiagnostic::CODE_IO,
                    )
                };
                eprint!("{}\n", diag.to_line());
            }
        }
        std::process::exit(1);
    }
}

fn compile(
    input: &PathBuf,
    output: Option<PathBuf>,
    name: Option<String>,
    caps: Vec<String>,
    notes: bool,
) -> anyhow::Result<()> {
    use crush_lang_sdk::formats::{self, InputFormat};
    let permissions: Vec<&str> = caps.iter().map(|s| s.as_str()).collect();

    let program = match formats::detect_or_bail(input, notes)? {
        InputFormat::Cvms => {
            let source = std::fs::read_to_string(input)?;
            crush_lang_sdk::assemble(&source, Some(&permissions), name.as_deref())?
        }
        format @ (InputFormat::CasmJson | InputFormat::CasmBinary) => {
            let mut casm = formats::load_casm(input, format)?;
            for cap in &caps {
                if !casm.manifest.permissions.contains(cap) {
                    casm.manifest.permissions.push(cap.clone());
                }
            }
            let mut program = crush_lang_sdk::compile::casm_to_vm(&casm)?;
            if name.is_some() {
                program.manifest.name = name;
            }
            program
        }
        other => anyhow::bail!(
            "crush-compile assembles CVM1 assembly (.cvms) or CASM JSON (.casm); '{}' is {other:?} \
             — compile it with crushc",
            input.display()
        ),
    };
    let blob = program.to_blob();

    if output.as_deref() == Some(std::path::Path::new("-")) {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        out.write_all(&blob)?;
        out.flush()?;
        return Ok(());
    }
    let out_path = output.unwrap_or_else(|| formats::artifact_path(input, "cvm1"));
    std::fs::write(&out_path, &blob)?;
    // The size written, not the code section's (it printed 15 for a 124-byte file).
    println!(
        "compiled {} → {} ({} bytes)",
        input.display(),
        out_path.display(),
        blob.len()
    );
    Ok(())
}
