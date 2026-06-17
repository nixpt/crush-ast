//! crush-lang-rust — Rust language support for Crush.
//!
//! Uses `syn` to parse Rust source and lower it to CAST.

pub mod lower_expr;
pub mod lower_stmt;
pub mod parser;

use std::any::Any;
use std::collections::HashMap;

use crush_cast::{Program, Function, Statement};
use walker_core::{FeatureReport, Frontend};

pub struct RustFrontend;

impl Frontend for RustFrontend {
    fn language_name(&self) -> &'static str { "rust" }
    fn file_extensions(&self) -> &[&'static str] { &[".rs"] }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        Ok(Box::new(parser::parse_source(source)?))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let file = ast.downcast_ref::<syn::File>()
            .ok_or_else(|| anyhow::anyhow!("expected syn::File"))?;
        let mut r = FeatureReport::default();
        r.lang = "rust".to_string();
        for item in &file.items {
            match item {
                syn::Item::Fn(_) => r.uses_functions = true,
                syn::Item::Struct(_) | syn::Item::Impl(_) | syn::Item::Trait(_) => {
                    r.uses_classes = true;
                }
                syn::Item::Use(_) | syn::Item::ExternCrate(_) => {
                    r.uses_imports.push(format!("{:?}", item));
                }
                syn::Item::ForeignMod(..) => r.uses_ffi = true,
                _ => {}
            }
            r.estimated_complexity += 1;
        }
        Ok(r)
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let file = ast.downcast::<syn::File>()
            .map_err(|_| anyhow::anyhow!("expected syn::File"))?;
        file_to_cast(*file)
    }
}

/// Parse Rust source and lower to CAST (convenience wrapper).
pub fn rust_to_cast(source: &str) -> anyhow::Result<Program> {
    let (_, program) = walker_core::frontend_pipeline(&RustFrontend, source)?;
    Ok(program)
}

fn file_to_cast(file: syn::File) -> anyhow::Result<Program> {
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
            _ => {
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
