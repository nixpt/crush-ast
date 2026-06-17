use std::fs;
use std::io::Read;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let source = if args.len() > 1 && args[1] != "--stdin" {
        let path = &args[1];
        fs::read_to_string(path)?
    } else {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    };

    let ext = args.get(1)
        .filter(|a| *a != "--stdin")
        .and_then(|p| Path::new(p).extension().and_then(|e| e.to_str()))
        .or_else(|| args.iter().position(|a| a == "--lang")
            .and_then(|i| args.get(i + 1).map(|s| s.as_str())))
        .unwrap_or("js");

    let program = crush_lang_js::js_to_cast(&source, ext)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}
