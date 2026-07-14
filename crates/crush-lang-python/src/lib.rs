//! crush-lang-python — Python language support for Crush.
//!
//! Uses `rustpython-parser` to parse Python source into a typed AST,
//! then lowers it to CAST for compilation through CrushVM.

pub mod analyzer;
pub mod lower_expr;
pub mod lower_stmt;
pub mod parser;
pub mod sdk;

use std::any::Any;
use std::collections::HashMap;

use crush_cast::{Function, Program, Statement};
use rustpython_ast as py_ast;
use walker_core::{FeatureReport, Frontend, LowerCtx};

/// Python language frontend implementing the `Frontend` trait.
pub struct PythonFrontend;

impl Frontend for PythonFrontend {
    fn language_name(&self) -> &'static str {
        "python"
    }
    fn file_extensions(&self) -> &[&'static str] {
        &[".py", ".pyi"]
    }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        let stmts = parser::parse_source(source)?;
        Ok(Box::new((source.to_string(), stmts)))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let stmts = ast
            .downcast_ref::<(String, Vec<py_ast::Stmt>)>()
            .map(|(_, stmts)| stmts)
            .ok_or_else(|| anyhow::anyhow!("expected (String, Stmt vec)"))?;
        let mut r = FeatureReport::default();
        r.lang = "python".to_string();
        for stmt in stmts {
            match stmt {
                py_ast::Stmt::FunctionDef { .. } => r.uses_functions = true,
                py_ast::Stmt::ClassDef { .. } => r.uses_classes = true,
                py_ast::Stmt::AsyncFunctionDef { .. } => r.uses_async = true,
                py_ast::Stmt::Try { .. } | py_ast::Stmt::Raise { .. } => r.uses_exceptions = true,
                py_ast::Stmt::Import(py_ast::StmtImport { names, .. }) => {
                    for alias in names {
                        r.uses_imports.push(alias.name.to_string());
                        if analyzer::is_dangerous_import(&alias.name.to_string()) {
                            r.dangerous_imports.push(alias.name.to_string());
                        }
                    }
                }
                py_ast::Stmt::ImportFrom(py_ast::StmtImportFrom { module, .. }) => {
                    if let Some(module) = module {
                        let m = module.to_string();
                        r.uses_imports.push(m.clone());
                        if analyzer::is_dangerous_import(&m) {
                            r.dangerous_imports.push(m);
                        }
                    }
                }
                py_ast::Stmt::Global { .. } | py_ast::Stmt::Nonlocal { .. } => {
                    r.uses_meta_programming = true;
                }
                _ => {}
            }
            r.estimated_complexity += 1;
        }
        Ok(r)
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let (source, stmts) = *ast
            .downcast::<(String, Vec<py_ast::Stmt>)>()
            .map_err(|_| anyhow::anyhow!("expected (String, Stmt vec)"))?;
        stmts_to_cast(stmts, &source)
    }
}

/// Parse Python source and lower to CAST (convenience wrapper).
pub fn python_to_cast(source: &str) -> anyhow::Result<Program> {
    let (_, program) = walker_core::frontend_pipeline(&PythonFrontend, source)?;
    Ok(program)
}

fn stmts_to_cast(stmts: Vec<py_ast::Stmt>, source: &str) -> anyhow::Result<Program> {
    let ctx = LowerCtx::new(source, "<crush>", "python");
    let mut main_body = Vec::new();
    let mut functions: HashMap<String, Function> = HashMap::new();

    for stmt in &stmts {
        match stmt {
            py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef { .. }) => {
                let lowered = lower_stmt::lower_stmt(stmt, &ctx)?;
                if let Statement::FunctionDef {
                    name: fn_name,
                    params,
                    body,
                    ..
                } = lowered
                {
                    functions.insert(
                        fn_name,
                        Function {
                            params,
                            body,
                            meta: HashMap::new(),
                            ..Default::default()
                        },
                    );
                }
            }
            py_ast::Stmt::AsyncFunctionDef(py_ast::StmtAsyncFunctionDef { .. }) => {
                let lowered = lower_stmt::lower_stmt(stmt, &ctx)?;
                if let Statement::FunctionDef {
                    name: fn_name,
                    params,
                    body,
                    ..
                } = lowered
                {
                    functions.insert(
                        fn_name,
                        Function {
                            params,
                            body,
                            meta: HashMap::new(),
                            is_async: true,
                            ..Default::default()
                        },
                    );
                }
            }
            _ => {}
        }
    }

    for stmt in &stmts {
        match stmt {
            py_ast::Stmt::FunctionDef { .. } | py_ast::Stmt::AsyncFunctionDef { .. } => {}
            _ => {
                main_body.push(lower_stmt::lower_stmt(stmt, &ctx)?);
            }
        }
    }

    if !main_body.is_empty() {
        functions.insert(
            "main".to_string(),
            Function {
                params: vec![],
                body: main_body,
                meta: HashMap::new(),
                ..Default::default()
            },
        );
    }

    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("python".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use walker_core::impl_adapter_from_frontend;

impl_adapter_from_frontend!(
    PythonAdapter,
    "python",
    &["py", "pyw"],
    crate::python_to_cast
);
