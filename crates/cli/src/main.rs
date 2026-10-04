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
        "go" => Some("go_walker"),
        "zig" => Some("zig_walker"),
        "sh" | "bash" => Some("bash_walker"),
        "zsh" => Some("zsh_walker"),
        "wasm" => Some("wasm_walker"),
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

#[cfg(test)]
mod tests {
    use super::walker_binary;

    /// Every binary `walker` dispatches to must be a `[[bin]]` some workspace
    /// crate builds (the go/zig/wasm entries once named binaries that didn't exist).
    #[test]
    fn every_walker_binary_is_built_by_the_workspace() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let manifests: String = std::fs::read_dir(crates)
            .unwrap()
            .filter_map(|e| std::fs::read_to_string(e.unwrap().path().join("Cargo.toml")).ok())
            .collect();
        let exts = [
            "rs", "py", "pyw", "js", "mjs", "cjs", "ts", "tsx", "mts", "c", "h", "cpp", "cc",
            "cxx", "c++", "hpp", "go", "zig", "sh", "bash", "zsh", "wasm",
        ];
        for ext in exts {
            let bin = walker_binary(ext).unwrap();
            let declared = format!("[[bin]]\nname = \"{bin}\"");
            assert!(
                manifests.contains(&declared),
                ".{ext} dispatches to `{bin}`, which no workspace crate declares as a [[bin]]"
            );
        }
    }
}
