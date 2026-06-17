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
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = compile(&cli.input, cli.output, cli.name, cli.caps) {
        eprintln!("crush-compile: {e:#}");
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
