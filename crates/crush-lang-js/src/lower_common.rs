//! CAST helpers shared by the swc and boa lowerings (parser-independent).

#![cfg_attr(not(any(feature = "swc-backend", feature = "js-boa")), allow(dead_code))]

use crush_cast::Statement;

/// Make every `continue` that belongs to the enclosing loop run `prefix` first
/// (a C-style `for`'s update, a `do-while`'s test). Nested loops and functions
/// own their own `continue`s and are left alone.
pub(crate) fn rewrite_continue(stmts: Vec<Statement>, prefix: &[Statement]) -> Vec<Statement> {
    if prefix.is_empty() {
        return stmts;
    }
    let mut out = Vec::with_capacity(stmts.len());
    for s in stmts {
        match s {
            Statement::Continue { meta } => {
                out.extend(prefix.iter().cloned());
                out.push(Statement::Continue { meta });
            }
            Statement::If { condition, then_body, else_body, meta } => out.push(Statement::If {
                condition,
                then_body: rewrite_continue(then_body, prefix),
                else_body: else_body.map(|e| rewrite_continue(e, prefix)),
                meta,
            }),
            Statement::TryCatch { body, error_var, handler, meta } => out.push(Statement::TryCatch {
                body: rewrite_continue(body, prefix),
                error_var,
                handler: rewrite_continue(handler, prefix),
                meta,
            }),
            other => out.push(other),
        }
    }
    out
}
