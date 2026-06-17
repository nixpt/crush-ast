use anyhow::{Context, Result};
use boa_ast::scope::Scope;
use boa_ast::Script;
use boa_ast::Module;
use boa_interner::Interner;
use boa_parser::Source;

pub enum BoaAst {
    Script(Script, Interner),
    Module(Module, Interner),
}

pub fn parse(source: &str) -> Result<BoaAst> {
    let mut interner = Interner::default();
    let scope = Scope::new_global();
    let mut parser = boa_parser::Parser::new(Source::from_bytes(source));
    match parser.parse_module(&scope, &mut interner) {
        Ok(module) => Ok(BoaAst::Module(module, interner)),
        Err(_) => {
            let mut interner = Interner::default();
            let mut parser = boa_parser::Parser::new(Source::from_bytes(source));
            let script = parser
                .parse_script(&Scope::new_global(), &mut interner)
                .context("boa parse error")?;
            Ok(BoaAst::Script(script, interner))
        }
    }
}
