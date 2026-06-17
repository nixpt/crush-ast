//! Python source parser — wraps rustpython-parser.

use rustpython_ast as ast;
use rustpython_parser::Parse;

/// Parse Python source code into a list of statements.
pub fn parse_source(source: &str) -> anyhow::Result<Vec<ast::Stmt>> {
    match ast::Suite::parse(source, "<crush>") {
        Ok(stmts) => Ok(stmts),
        Err(e) => anyhow::bail!("Python syntax error: {}", e),
    }
}

/// Parse a single Python expression.
pub fn parse_expression(source: &str) -> anyhow::Result<ast::Expr> {
    match ast::Expr::parse(source, "<crush>") {
        Ok(expr) => Ok(expr),
        Err(e) => anyhow::bail!("Python expression error: {}", e),
    }
}
