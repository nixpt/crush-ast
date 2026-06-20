//! `crush-compile` — compile CRUSH/CASM text to CVM1 binary bytecode.
//!
//! # Examples
//!
//! ```bash
//! crush-compile hello.casm -o hello.cvm1
//! crush-compile hello.casm -o hello.cvm1 --cap io.print --cap fs.read
//! crush-compile hello.casm --name hello_world
//! ```

use std::path::PathBuf;

use clap::Parser;

use crush_lang_sdk::MessageFormat;

#[derive(Parser)]
#[command(name = "crush-compile")]
#[command(about = "Compile CRUSH/CASM text to CVM1 bytecode")]
struct Cli {
    /// Input CASM text file.
    input: PathBuf,

    /// Output CVM1 binary file.
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
    if let Err(e) = compile(&cli.input, cli.output, cli.name, cli.caps) {
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
) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(input)?;
    let permissions: Vec<&str> = caps.iter().map(|s| s.as_str()).collect();

    let program = crush_lang_sdk::assemble(&source, Some(&permissions), name.as_deref())?;
    let blob = program.to_blob();

    let out_path = output.unwrap_or_else(|| {
        let mut p = input.clone();
        p.set_extension("cvm1");
        p
    });

    std::fs::write(&out_path, blob)?;
    println!(
        "compiled {} → {} ({} bytes)",
        input.display(),
        out_path.display(),
        program.code.len()
    );
    Ok(())
}
