use anyhow::{Context, Result};
use boa_ast::scope::Scope;
use boa_ast::Script;
use boa_interner::Interner;
use boa_parser::Source;

pub fn parse(source: &str) -> Result<(Script, Interner)> {
    let mut interner = Interner::default();
    let scope = Scope::new_global();
    let source = Source::from_bytes(source);
    let mut parser = boa_parser::Parser::new(source);
    let script = parser
        .parse_script(&scope, &mut interner)
        .context("boa parse error")?;
    Ok((script, interner))
}
