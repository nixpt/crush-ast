//! # Zig Walker
//!
//! Transforms Zig source code into CAST (Crush Abstract Syntax Tree).
//!
//! ## Classification
//! This is a **walker/transpiler**, not a runtime. It converts Zig source
//! to CAST which is then compiled to CASM and executed on the CRUSH VM.

use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use crush_cast::{self as ast, CastType, Expression, Statement};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tree_sitter::{Node, Parser, Tree};

#[derive(ClapParser)]
#[command(name = "zig_walker")]
#[command(about = "Transform Zig source code to CAST")]
struct Cli {
    /// Input Zig source file
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source_code = fs::read_to_string(&cli.input)?;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .context("Error loading Zig grammar")?;

    let tree = parser
        .parse(&source_code, None)
        .context("Error parsing Zig source code")?;

    let program = walk_tree(&tree, source_code.as_bytes(), &cli.input)?;
    println!("{}", serde_json::to_string_pretty(&program)?);

    Ok(())
}

fn walk_tree(tree: &Tree, source: &[u8], file_name: &str) -> Result<ast::Program> {
    let root_node = tree.root_node();
    let mut functions = HashMap::new();
    let mut main_body = Vec::new();

    for child in root_node.children(&mut root_node.walk()) {
        if let Some(stmt) = visit_top_level(child, source, file_name, &mut functions)? {
            main_body.push(stmt);
        }
    }

    if !functions.contains_key("main") {
        functions.insert(
            "main".to_string(),
            ast::Function {
                params: vec![],
                body: main_body,
                meta: HashMap::new(),
                ..Default::default()
            },
        );
    }

    Ok(ast::Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("zig".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

fn text<'a>(node: Node<'a>, source: &'a [u8]) -> Result<&'a str> {
    std::str::from_utf8(&source[node.byte_range()]).context("Invalid UTF-8")
}

fn create_meta(node: Node, file_name: &str) -> HashMap<String, serde_json::Value> {
    let mut meta = HashMap::new();
    meta.insert("file".to_string(), json!(file_name));
    meta.insert("line".to_string(), json!(node.start_position().row + 1));
    meta.insert("lang".to_string(), json!("zig"));
    meta
}

fn visit_top_level(
    node: Node,
    source: &[u8],
    file_name: &str,
    functions: &mut HashMap<String, ast::Function>,
) -> Result<Option<Statement>> {
    let meta = create_meta(node, file_name);

    match node.kind() {
        "FnDecl" | "fn_decl" | "function_declaration" => {
            visit_function(node, source, file_name, functions)?;
            Ok(None)
        }
        "VarDecl" | "var_decl" | "const_decl" | "variable_declaration" => {
            visit_var_decl(node, source, file_name, meta)
        }
        _ => Ok(None),
    }
}

fn visit_function(
    node: Node,
    source: &[u8],
    file_name: &str,
    functions: &mut HashMap<String, ast::Function>,
) -> Result<()> {
    let meta = create_meta(node, file_name);

    let name = node
        .child_by_field_name("name")
        .or_else(|| {
            node.children(&mut node.walk())
                .find(|c| c.kind() == "identifier")
        })
        .map(|n| text(n, source).unwrap_or("anonymous").to_string())
        .unwrap_or_else(|| "anonymous".to_string());

    let params = visit_params(node, source)?;
    let body = node
        .child_by_field_name("body")
        .map(|b| visit_block(b, source, file_name))
        .transpose()?
        .unwrap_or_default();

    functions.insert(
        name,
        ast::Function {
            params,
            body,
            meta,
            ..Default::default()
        },
    );
    Ok(())
}

fn visit_params(node: Node, source: &[u8]) -> Result<Vec<(String, CastType)>> {
    let mut params = Vec::new();
    if let Some(params_node) = node.child_by_field_name("params") {
        for child in params_node.children(&mut params_node.walk()) {
            if matches!(child.kind(), "param" | "Param" | "parameter") {
                if let Some(name_node) = child.child_by_field_name("name") {
                    params.push((text(name_node, source)?.to_string(), CastType::Any));
                }
            }
        }
    }
    Ok(params)
}

fn visit_block(node: Node, source: &[u8], file_name: &str) -> Result<Vec<Statement>> {
    node.children(&mut node.walk())
        .filter_map(|child| visit_statement(child, source, file_name).transpose())
        .collect()
}

fn visit_statement(node: Node, source: &[u8], file_name: &str) -> Result<Option<Statement>> {
    let meta = create_meta(node, file_name);

    match node.kind() {
        "VarDecl" | "var_decl" | "variable_declaration" => {
            visit_var_decl(node, source, file_name, meta)
        }
        "ReturnStatement" | "return_statement" => {
            let value = node
                .child_by_field_name("value")
                .map(|n| visit_expression(n, source, file_name))
                .transpose()?;
            Ok(Some(Statement::Return { value, meta }))
        }
        "BreakStatement" | "break_statement" => Ok(Some(Statement::Break { meta })),
        "ContinueStatement" | "continue_statement" => Ok(Some(Statement::Continue { meta })),
        _ => {
            if node.child_count() > 0 {
                if let Ok(expr) = visit_expression(node, source, file_name) {
                    return Ok(Some(Statement::ExprStmt { expr, meta }));
                }
            }
            Ok(None)
        }
    }
}

fn visit_var_decl(
    node: Node,
    source: &[u8],
    file_name: &str,
    meta: HashMap<String, serde_json::Value>,
) -> Result<Option<Statement>> {
    let name = node
        .child_by_field_name("name")
        .map(|n| text(n, source).unwrap_or("_").to_string())
        .unwrap_or_else(|| "_".to_string());

    let value = node
        .child_by_field_name("value")
        .map(|n| visit_expression(n, source, file_name))
        .transpose()?
        .unwrap_or(Expression::NullLiteral { meta: meta.clone() });

    Ok(Some(Statement::VarDecl {
        name,
        value,
        type_hint: CastType::Any,
        meta,
    }))
}

fn visit_expression(node: Node, source: &[u8], file_name: &str) -> Result<Expression> {
    let meta = create_meta(node, file_name);

    match node.kind() {
        "Identifier" | "identifier" => Ok(Expression::Var {
            name: text(node, source)?.to_string(),
            meta,
        }),
        "IntegerLiteral" | "integer_literal" => Ok(Expression::IntLiteral {
            value: text(node, source)?.parse().unwrap_or(0),
            meta,
        }),
        "StringLiteral" | "string_literal" => Ok(Expression::StringLiteral {
            value: text(node, source)?.trim_matches('"').to_string(),
            meta,
        }),
        "true" => Ok(Expression::BoolLiteral { value: true, meta }),
        "false" => Ok(Expression::BoolLiteral { value: false, meta }),
        "null" => Ok(Expression::NullLiteral { meta }),
        "BinaryExpr" | "binary_expr" if node.child_count() >= 3 => {
            let left = Box::new(visit_expression(node.child(0).unwrap(), source, file_name)?);
            let operator = text(node.child(1).unwrap(), source)?.to_string();
            let right = Box::new(visit_expression(node.child(2).unwrap(), source, file_name)?);
            Ok(Expression::BinaryOp {
                operator,
                left,
                right,
                meta,
            })
        }
        "CallExpr" | "call_expr" => {
            let func_node = node
                .child_by_field_name("function")
                .or(node.child(0))
                .unwrap();
            let func_name = text(func_node, source)?.to_string();

            let mut args = Vec::new();
            if let Some(args_node) = node.child_by_field_name("args") {
                for arg in args_node.children(&mut args_node.walk()) {
                    if !matches!(arg.kind(), "(" | ")" | ",") {
                        if let Ok(expr) = visit_expression(arg, source, file_name) {
                            args.push(expr);
                        }
                    }
                }
            }

            // Use standard capability mapping
            match func_name.as_str() {
                "print" | "std.debug.print" => Ok(Expression::CapabilityCall {
                    name: "io.print".to_string(), // Standard capability name
                    args,
                    meta: {
                        let mut m = meta;
                        m.insert("capability".to_string(), json!(true));
                        m
                    },
                }),
                _ => Ok(Expression::Call {
                    function: func_name,
                    args,
                    meta,
                }),
            }
        }
        _ => {
            if node.child_count() == 1 {
                visit_expression(node.child(0).unwrap(), source, file_name)
            } else {
                Ok(Expression::NullLiteral { meta })
            }
        }
    }
}
