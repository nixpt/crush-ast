//! crush-lang-python — Python language support for Crush.
//!
//! Uses `rustpython-parser` to parse Python source into a typed AST,
//! then lowers it to CAST (Crush Abstract Syntax Tree) for compilation
//! through the existing `crush_frontend` → CASM → CrushVM pipeline.

pub mod analyzer;
pub mod lower_expr;
pub mod lower_stmt;
pub mod parser;

use std::collections::HashMap;

use crush_cast::{Program, Function, Statement};
use rustpython_ast as py_ast;

/// Parse Python source and lower it to a CAST Program.
pub fn python_to_cast(source: &str) -> anyhow::Result<Program> {
    let stmts = parser::parse_source(source)?;

    let mut main_body = Vec::new();
    let mut functions: HashMap<String, Function> = HashMap::new();

    // First pass: collect function definitions
    for stmt in &stmts {
        if let py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef { .. }) = stmt {
            let lowered = lower_stmt::lower_stmt(stmt)?;
            if let Statement::FunctionDef { name: fn_name, params, body, .. } = lowered {
                functions.insert(fn_name, Function { params, body, meta: HashMap::new() });
            }
        }
    }

    // Second pass: lower everything else to main body
    for stmt in &stmts {
        match stmt {
            py_ast::Stmt::FunctionDef { .. } | py_ast::Stmt::AsyncFunctionDef { .. } => {}
            _ => {
                main_body.push(lower_stmt::lower_stmt(stmt)?);
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
        lang: Some("python".to_string()),
        functions,
        ai_meta: None,
    })
}
