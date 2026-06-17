//! rust_walker — Crush Rust to CAST transpiler.
//!
//! Uses syn to parse Rust source and lower to CAST JSON.
//! Usage: rust_walker <file.rs>

use std::fs;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: rust_walker <file.rs>");
    let source = fs::read_to_string(path)?;

    let program = crush_lang_rust::rust_to_cast(&source)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}
