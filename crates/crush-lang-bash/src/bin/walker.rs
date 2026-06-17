use std::fs;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: bash_walker <file.sh>");
    let source = fs::read_to_string(path)?;

    let program = crush_lang_bash::bash_to_cast(&source)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}
