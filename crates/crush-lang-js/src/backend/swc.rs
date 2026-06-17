use anyhow::Result;
use swc_common::{FileName, SourceMap};
use swc_ecma_ast::Module;
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax};

pub fn parse(source: &str, ext: &str) -> Result<Module> {
    let is_tsx = ext == "tsx";
    let is_ts = ext == "ts" || ext == "tsx" || ext == "mts";
    let is_jsx = ext == "jsx" || ext == "tsx";

    let syntax = if is_ts {
        Syntax::Typescript(TsSyntax {
            tsx: is_tsx,
            decorators: true,
            dts: false,
            no_early_errors: false,
            disallow_ambiguous_jsx_like: false,
            ..Default::default()
        })
    } else {
        Syntax::Es(EsSyntax {
            jsx: is_jsx,
            ..Default::default()
        })
    };

    let cm = SourceMap::default();
    let fm = cm.new_source_file(
        FileName::Custom(format!("input.{}", ext)).into(),
        source.to_string(),
    );

    let mut parser = Parser::new(syntax, StringInput::from(&*fm), None);
    let module = parser
        .parse_module()
        .map_err(|e| anyhow::anyhow!("swc parse error: {:?}", e))?;

    Ok(module)
}
