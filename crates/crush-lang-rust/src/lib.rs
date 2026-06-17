//! crush-lang-rust — Rust language support for Crush.
//!
//! Uses `syn` to parse Rust source and lower it to CAST for compilation
//! through the existing CrushVM pipeline.

pub mod lower_expr;
pub mod lower_stmt;
pub mod parser;

use std::collections::HashMap;

use crush_cast::{Program, Function, Statement};

/// Parse Rust source and lower it to a CAST Program.
pub fn rust_to_cast(source: &str) -> anyhow::Result<Program> {
    let file = parser::parse_source(source)?;

    let mut main_body = Vec::new();
    let mut functions: HashMap<String, Function> = HashMap::new();

    for item in &file.items {
        match item {
            syn::Item::Fn(_) => {
                let lowered = lower_stmt::lower_stmt(&syn::Stmt::Item(item.clone()))?;
                if let Statement::FunctionDef { name, params, body, .. } = lowered {
                    functions.insert(name, Function { params, body, meta: HashMap::new() });
                }
            }
            syn::Item::Macro(mac) => {
                // println!() macro calls at top level become expression statements
                let tokens = &mac.mac.tokens;
                let src = tokens.to_string();
                if src.starts_with("println") || src.starts_with("print") {
                    // For simplicity, macro invocations at top level are skipped
                    // They should be in function bodies
                }
            }
            _ => {
                // Non-function items at top level go to main body
                let stmt = syn::Stmt::Item(item.clone());
                main_body.push(lower_stmt::lower_stmt(&stmt)?);
            }
        }
    }

    if !main_body.is_empty() {
        functions.insert("main".to_string(), Function {
            params: vec![],
            body: main_body,
            meta: HashMap::new(),
        });
    }

    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("rust".to_string()),
        functions,
        ai_meta: None,
    })
}
