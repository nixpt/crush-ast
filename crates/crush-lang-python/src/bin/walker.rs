//! python_walker — Crush Python to CAST transpiler.
//!
//! Reads a Python source file, parses it with rustpython-parser,
//! lowers the AST to CAST JSON, and prints the result to stdout.
//!
//! Usage: python_walker <file.py>

use anyhow::Result;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: python_walker <file.py>");
    let source = fs::read_to_string(path)?;

    let program = crush_lang_python::python_to_cast(&source)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}
