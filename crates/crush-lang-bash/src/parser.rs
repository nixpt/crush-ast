use anyhow::{Context, Result};
use std::io::BufReader;

pub fn parse_source(source: &str) -> Result<brush_parser::ast::Program> {
    let opts = brush_parser::ParserOptions {
        enable_extended_globbing: true,
        posix_mode: false,
        sh_mode: true,
        tilde_expansion_at_word_start: true,
        tilde_expansion_after_colon: false,
        parser_impl: brush_parser::ParserImpl::default(),
    };
    let reader = BufReader::new(source.as_bytes());
    let mut parser = brush_parser::Parser::new(reader, &opts);
    let program = parser
        .parse_program()
        .context("Failed to parse shell script")?;
    Ok(program)
}
