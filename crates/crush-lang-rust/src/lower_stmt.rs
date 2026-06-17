//! syn statement → CAST statement lowering.

use std::collections::HashMap;

use crush_cast::{CastType, Expression, Statement};
use syn::Stmt;

use crate::lower_expr::{lower_expr, pat_to_ident};

pub fn lower_stmt(stmt: &Stmt) -> anyhow::Result<Statement> {
    let meta = HashMap::new();
    match stmt {
        Stmt::Local(local) => {
            let name = pat_to_ident(&local.pat)?;
            let value = match &local.init {
                Some(init) => lower_expr(&init.expr)?,
                None => Expression::NullLiteral { meta: meta.clone() },
            };
            Ok(Statement::VarDecl {
                name,
                value,
                type_hint: CastType::Any,
                meta,
            })
        }
        Stmt::Item(item) => match item {
            syn::Item::Fn(item_fn) => {
                let name = item_fn.sig.ident.to_string();
                let params: Vec<(String, CastType)> = item_fn
                    .sig
                    .inputs
                    .iter()
                    .map(|p| match p {
                        syn::FnArg::Typed(pat_type) => {
                            let name = match pat_type.pat.as_ref() {
                                syn::Pat::Ident(pi) => pi.ident.to_string(),
                                _ => "_".to_string(),
                            };
                            (name, CastType::Any)
                        }
                        _ => ("_".to_string(), CastType::Any),
                    })
                    .collect();
                let mut body = Vec::new();
                for s in &item_fn.block.stmts {
                    body.push(lower_stmt(s)?);
                }
                Ok(Statement::FunctionDef {
                    name,
                    params,
                    body,
                    meta,
                })
            }
            _ => anyhow::bail!("unsupported item"),
        },
        Stmt::Expr(expr, _) => {
            let expr = lower_expr(expr)?;
            Ok(Statement::ExprStmt { expr, meta })
        }
        Stmt::Macro(mac) => {
            let mac_name = mac
                .mac
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            match mac_name.as_str() {
                "println" | "print" => Ok(Statement::ExprStmt {
                    expr: Expression::CapabilityCall {
                        name: "io.print".to_string(),
                        args: vec![],
                        meta,
                    },
                    meta: HashMap::new(),
                }),
                _ => anyhow::bail!("macro invocation not supported: {}", mac_name),
            }
        }
        _ => anyhow::bail!("unsupported Rust statement"),
    }
}
