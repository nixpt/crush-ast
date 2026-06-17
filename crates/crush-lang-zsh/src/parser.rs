use anyhow::{Context, Result};

pub fn parse_source(source: &str) -> Result<zshrs_parse::parser::ZshProgram> {
    let mut parser = zshrs_parse::parser::ZshParser::new(source);
    let program = parser
        .parse()
        .map_err(|errors| {
            let msgs: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            anyhow::anyhow!("Zsh parse errors: {}", msgs.join("; "))
        })
        .context("Failed to parse zsh script")?;
    Ok(program)
}
