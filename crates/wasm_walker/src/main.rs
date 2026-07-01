use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use wasm_walker::walk_wasm;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to WASM file
    #[arg(value_name = "FILE")]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let wasm_bytes =
        fs::read(&cli.file).with_context(|| format!("Failed to read WASM file: {:?}", cli.file))?;

    let program = walk_wasm(&wasm_bytes, cli.file.to_str().unwrap())?;
    println!("{}", serde_json::to_string_pretty(&program)?);

    Ok(())
}
