use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Source file to walk
    file: PathBuf,

    /// Output format (json, casm)
    #[arg(short, long, default_value = "json")]
    format: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let extension = cli
        .file
        .extension()
        .and_then(|s| s.to_str())
        .context("File has no extension")?;

    println!("Walker observing: {:?} (Type: {})", cli.file, extension);

    match extension {
        "rs" => println!("Delegating to rust_walker..."),
        "py" => println!("Delegating to python_walker..."),
        "js" | "ts" => println!("Delegating to js_walker..."),
        "c" | "h" => println!("Delegating to c_walker..."),
        "go" => println!("Delegating to go_walker..."),
        "crush" => println!("Delegating to crush_walker..."),
        "zig" => println!("Delegating to zig_walker..."),
        "sh" => println!("Delegating to bash_walker..."),
        _ => println!(
            "Unknown extension '{}', using fallback generic walker.",
            extension
        ),
    }

    Ok(())
}
