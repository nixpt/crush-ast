use std::fs;
use std::path::Path;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: js_walker <file.(js|ts|jsx|tsx)>");
    let source = fs::read_to_string(path)?;

    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("js");

    let program = crush_lang_js::js_to_cast(&source, ext)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}
