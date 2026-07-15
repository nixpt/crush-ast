use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Source file to walk
    file: PathBuf,

    /// Output format (json, casm)
    #[arg(short, long, default_value = "json")]
    format: String,
}

fn walker_binary(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust_walker"),
        "py" | "pyw" => Some("python_walker"),
        "js" | "mjs" | "cjs" => Some("js_walker"),
        "ts" | "tsx" | "mts" => Some("js_walker"),
        "c" | "h" | "cpp" | "cc" | "cxx" | "c++" | "hpp" => Some("c_walker"),
        "go" => Some("crush_lang_go"),
        "zig" => Some("crush_lang_zig"),
        "sh" | "bash" => Some("bash_walker"),
        "zsh" => Some("zsh_walker"),
        "wasm" => Some("crush_lang_wasm"),
        _ => None,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let extension = cli
        .file
        .extension()
        .and_then(|s| s.to_str())
        .context("File has no extension")?;

    let binary = walker_binary(extension)
        .ok_or_else(|| anyhow::anyhow!("Unknown extension '.{extension}', no walker available"))?;

    let output = Command::new(binary)
        .arg(&cli.file)
        .output()
        .with_context(|| format!("Failed to execute {binary}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{binary} failed:\n{stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    Ok(())
}
