use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use crush_cast::{self as ast, CastType, Expression, Statement};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tree_sitter::{Node, Parser, Tree};
use walker_core::{BaseWalker, Walker};

#[derive(ClapParser)]
#[command(name = "python_walker")]
struct Cli {
    /// Input Python source file
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source_code = fs::read_to_string(&cli.input)?;

    let mut parser = Parser::new();
    let language: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    parser
        .set_language(&language)
        .context("Error loading Python grammar")?;

    let tree = parser
        .parse(&source_code, None)
        .context("Error parsing source code")?;

    let walker = PythonWalker {
        file_name: cli.input.to_string(),
    };

    let program = walker.walk(&tree, source_code.as_bytes())?;

    println!("{}", serde_json::to_string_pretty(&program)?);

    Ok(())
}

struct PythonWalker {
    file_name: String,
}

impl Walker for PythonWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn walk(&self, tree: &Tree, source: &[u8]) -> Result<ast::Program> {
        let root_node = tree.root_node();
        let mut functions = HashMap::new();
        let mut main_body = Vec::new();
        let mut imports = Vec::new();

        let mut visitor = Visitor {
            base: BaseWalker::new(source),
            functions: &mut functions,
            imports: &mut imports,
            file_name: &self.file_name,
        };

        for child in root_node.children(&mut root_node.walk()) {
            if let Some(stmt) = visitor.visit_statement(child)? {
                main_body.push(stmt);
            }
        }

        functions.insert(
            "main".to_string(),
            ast::Function {
                params: vec![],
                body: main_body,
                meta: HashMap::new(),
            },
        );

        Ok(ast::Program {
            cast_version: "0.2".to_string(),
            entry: "main".to_string(),
            lang: Some("python".to_string()),
            functions,
            ai_meta: None,
        })
    }
}

struct Visitor<'a> {
    base: BaseWalker<'a>,
    functions: &'a mut HashMap<String, ast::Function>,
    imports: &'a mut Vec<String>,
    file_name: &'a str,
}

impl<'a> Visitor<'a> {
    fn visit_statement(&mut self, node: Node) -> Result<Option<Statement>> {
        let meta = self.base.create_meta(node, "python", self.file_name);
        match node.kind() {
            "import_statement" | "import_from_statement" => {
                // Simplified import collection (match legacy logic)
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "dotted_name" {
                        self.imports.push(self.base.text(child)?.to_string());
                    }
                }
                Ok(None)
            }
            "function_definition" => {
                let name = self.base.child_text(node, "name")?.to_string();
                let params_node = node.child_by_field_name("parameters").unwrap();
                let mut params = Vec::new();
                for p_node in params_node.children(&mut params_node.walk()) {
                    if p_node.kind() == "identifier" {
                        params.push((self.base.text(p_node)?.to_string(), CastType::Any));
                    }
                }
                let body_node = node.child_by_field_name("body").unwrap();
                let body = self.visit_block(body_node)?;
                self.functions
                    .insert(name, ast::Function { params, body, meta });
                Ok(None)
            }
            "expression_statement" => {
                let child = node.child(0).unwrap();
                if child.kind() == "assignment" {
                    let left = child.child_by_field_name("left").unwrap();
                    let right = child.child_by_field_name("right").unwrap();
                    if left.kind() == "identifier" {
                        let name = self.base.text(left)?.to_string();
                        let value = self.visit_expression(right)?;
                        return Ok(Some(Statement::VarDecl {
                            name,
                            value,
                            type_hint: CastType::Any,
                            meta: self.base.create_meta(child, "python", self.file_name),
                        }));
                    }
                }

                let expr = self.visit_expression(child)?;

                // Special check for __crush_export__ call
                if let Expression::CapabilityCall {
                    ref name, ref args, ..
                } = expr
                {
                    if name == "__crush_export__" && args.len() == 2 {
                        if let Expression::StringLiteral {
                            value: export_name, ..
                        } = &args[0]
                        {
                            return Ok(Some(Statement::Export {
                                name: export_name.clone(),
                                value: args[1].clone(),
                                meta,
                            }));
                        }
                    }
                }

                Ok(Some(Statement::ExprStmt { expr, meta }))
            }
            "if_statement" => {
                let cond_node = node.child_by_field_name("condition").unwrap();
                let condition = self.visit_expression(cond_node)?;
                let cons_node = node.child_by_field_name("consequence").unwrap();
                let then_body = self.visit_block(cons_node)?;

                let mut else_body = None;
                if let Some(alt_node) = node.child_by_field_name("alternative") {
                    else_body = Some(self.visit_block(alt_node)?);
                }
                Ok(Some(Statement::If {
                    condition,
                    then_body,
                    else_body,
                    meta,
                }))
            }
            "while_statement" => {
                let cond_node = node.child_by_field_name("condition").unwrap();
                let condition = self.visit_expression(cond_node)?;
                let body_node = node.child_by_field_name("body").unwrap();
                let body = self.visit_block(body_node)?;
                Ok(Some(Statement::While {
                    condition: Box::new(condition),
                    body,
                    meta,
                }))
            }
            "return_statement" => {
                let value = if let Some(val_node) = node.child(1) {
                    if val_node.kind() != ";" {
                        Some(self.visit_expression(val_node)?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(Some(Statement::Return { value, meta }))
            }
            "block" => {
                // Should not happen as visit_block handles children
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn visit_block(&mut self, node: Node) -> Result<Vec<Statement>> {
        let mut body = Vec::new();
        for child in node.children(&mut node.walk()) {
            if let Some(stmt) = self.visit_statement(child)? {
                body.push(stmt);
            }
        }
        Ok(body)
    }

    fn visit_expression(&mut self, node: Node) -> Result<Expression> {
        let node = self.base.unwrap_parens(node);
        let meta = self.base.create_meta(node, "python", self.file_name);

        match node.kind() {
            "identifier" => Ok(Expression::Var {
                name: self.base.text(node)?.to_string(),
                meta,
            }),
            "integer" => Ok(Expression::IntLiteral {
                value: self.base.extract_int_literal(node)?,
                meta,
            }),
            "float" => Ok(Expression::FloatLiteral {
                value: self.base.extract_float_literal(node)?,
                meta,
            }),
            "string" => Ok(Expression::StringLiteral {
                value: self.base.extract_string_literal(node)?,
                meta,
            }),
            "true" => Ok(Expression::BoolLiteral { value: true, meta }),
            "false" => Ok(Expression::BoolLiteral { value: false, meta }),
            "none" => Ok(Expression::NullLiteral { meta }),
            "binary_operator" => {
                let left = Box::new(self.visit_expression(node.child(0).unwrap())?);
                let operator = self.base.text(node.child(1).unwrap())?.to_string();
                let right = Box::new(self.visit_expression(node.child(2).unwrap())?);
                Ok(Expression::BinaryOp {
                    operator,
                    left,
                    right,
                    meta,
                })
            }
            "comparison_operator" => {
                let left = Box::new(self.visit_expression(node.child(0).unwrap())?);
                let operator = self.base.text(node.child(1).unwrap())?.to_string();
                let right = Box::new(self.visit_expression(node.child(2).unwrap())?);
                Ok(Expression::BinaryOp {
                    operator,
                    left,
                    right,
                    meta,
                })
            }
            "call" => {
                let func_node = node.child_by_field_name("function").unwrap();
                let args_node = node.child_by_field_name("arguments").unwrap();
                let mut args = Vec::new();
                for arg in args_node.children(&mut args_node.walk()) {
                    if arg.kind() == "expression"
                        || arg.kind() == "identifier"
                        || arg.kind().ends_with("_literal")
                        || arg.kind() == "string"
                        || arg.kind() == "integer"
                    {
                        args.push(self.visit_expression(arg)?);
                    }
                }

                if func_node.kind() == "identifier" {
                    let func_name = self.base.text(func_node)?;

                    // Use centralized capability mapping
                    if let Some(cap_name) = walker_core::map_to_capability("python", func_name) {
                        return Ok(Expression::CapabilityCall {
                            name: cap_name.to_string(),
                            args,
                            meta: {
                                let mut m = meta;
                                m.insert("capability".to_string(), json!(true));
                                if let Some((ns, method)) = cap_name.split_once('.') {
                                    m.insert("namespace".to_string(), json!(ns));
                                    m.insert("method".to_string(), json!(method));
                                }
                                m
                            },
                        });
                    }

                    match func_name {
                        "__crush_import__" => {
                            let name = if let Some(Expression::StringLiteral { value, .. }) =
                                args.first()
                            {
                                value.clone()
                            } else {
                                "".to_string()
                            };
                            Ok(Expression::Call {
                                function: "__crush_import__".to_string(),
                                args: vec![Expression::StringLiteral {
                                    value: name,
                                    meta: meta.clone(),
                                }],
                                meta,
                            })
                        }
                        "__crush_export__" | "__crush_ffi__" | "__crush_call__" => {
                            Ok(Expression::CapabilityCall {
                                name: func_name.to_string(),
                                args,
                                meta,
                            })
                        }
                        _ => Ok(Expression::Call {
                            function: func_name.to_string(),
                            args,
                            meta,
                        }),
                    }
                } else {
                    Ok(Expression::NullLiteral { meta })
                }
            }
            "list" => {
                // Not strictly in CAST yet as a first-class expr, but we had it in legacy.
                // For now, let's just null it or stick to minimal.
                Ok(Expression::NullLiteral { meta })
            }
            _ => {
                // Fallback for expression wrapper types
                if node.child_count() == 1 {
                    self.visit_expression(node.child(0).unwrap())
                } else {
                    Ok(Expression::NullLiteral { meta })
                }
            }
        }
    }
}
