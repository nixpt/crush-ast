use std::fs;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: zsh_walker <file.zsh>");
    let source = fs::read_to_string(path)?;

    let program = crush_lang_zsh::zsh_to_cast(&source)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}
