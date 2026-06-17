use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use crush_cast::{self as ast, CastType, Expression, Statement};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tree_sitter::{Node, Parser, Tree};
use walker_core::{BaseWalker, Walker};

#[derive(ClapParser)]
#[command(name = "c_walker")]
struct Cli {
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source_code = fs::read_to_string(&cli.input)?;

    let mut parser = Parser::new();
    let language: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
    parser
        .set_language(&language)
        .context("Error loading C grammar")?;

    let tree = parser
        .parse(&source_code, None)
        .context("Error parsing source code")?;

    let walker = CWalker {
        file_name: cli.input.to_string(),
    };

    let program = walker.walk(&tree, source_code.as_bytes())?;
    println!("{}", serde_json::to_string_pretty(&program)?);

    Ok(())
}

struct CWalker {
    file_name: String,
}

impl Walker for CWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_c::LANGUAGE.into()
    }

    fn walk(&self, tree: &Tree, source: &[u8]) -> Result<ast::Program> {
        let root_node = tree.root_node();
        let mut functions = HashMap::new();
        let mut main_body = Vec::new();

        let mut visitor = Visitor {
            base: BaseWalker::new(source),
            functions: &mut functions,
            file_name: &self.file_name,
        };

        for child in root_node.children(&mut root_node.walk()) {
            if let Some(stmt) = visitor.visit_statement(child)? {
                main_body.push(stmt);
            }
        }

        if !functions.contains_key("main") || !main_body.is_empty() {
            functions
                .entry("main".to_string())
                .or_insert_with(|| ast::Function {
                    params: vec![],
                    body: Vec::new(),
                    meta: HashMap::new(),
                    ..Default::default()
                })
                .body
                .extend(main_body);
        }

        Ok(ast::Program {
            cast_version: "0.1".to_string(),
            entry: "main".to_string(),
            lang: Some("c".to_string()),
            functions,
            ai_meta: None,
            ..Default::default()
        })
    }
}

struct Visitor<'a> {
    base: BaseWalker<'a>,
    functions: &'a mut HashMap<String, ast::Function>,
    file_name: &'a str,
}

impl<'a> Visitor<'a> {
    fn visit_statement(&mut self, node: Node) -> Result<Option<Statement>> {
        let meta = self.base.create_meta(node, "c", self.file_name);
        match node.kind() {
            "function_definition" => {
                let decl = node.child_by_field_name("declarator").unwrap();
                let name = self.extract_name_from_declarator(decl)?.to_string();

                let mut params = Vec::new();
                if let Some(params_node) = decl.child_by_field_name("parameters") {
                    for p_decl in params_node.children(&mut params_node.walk()) {
                        if p_decl.kind() == "parameter_declaration" {
                            if let Some(p_var_decl) = p_decl.child_by_field_name("declarator") {
                                params
                                    .push((self.base.text(p_var_decl)?.to_string(), CastType::Any));
                            }
                        }
                    }
                }

                let body_node = node.child_by_field_name("body").unwrap();
                let body = self.visit_block(body_node)?;
                self.functions.insert(
                    name,
                    ast::Function {
                        params,
                        body,
                        meta,
                        ..Default::default()
                    },
                );
                Ok(None)
            }
            "declaration" => {
                // Minimal: handle simple int x = 10;
                if let Some(decl) = node.child_by_field_name("declarator") {
                    if decl.kind() == "init_declarator" {
                        let name_node = decl.child_by_field_name("declarator").unwrap();
                        let name = self.base.text(name_node)?.to_string();
                        let value_node = decl.child_by_field_name("value").unwrap();
                        let value = self.visit_expression(value_node)?;
                        return Ok(Some(Statement::VarDecl {
                            name,
                            value,
                            type_hint: CastType::Any,
                            meta,
                        }));
                    }
                }
                Ok(None)
            }
            "expression_statement" => {
                let expr_node = node.child(0).unwrap();
                let expr = self.visit_expression(expr_node)?;

                // Special check for __crush_export__ call
                if let Expression::Call {
                    ref function,
                    ref args,
                    ..
                } = expr
                {
                    if function == "__crush_export__" && args.len() == 2 {
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
            "return_statement" => {
                let value = if let Some(expr) = node.child(1) {
                    if expr.kind() != ";" {
                        Some(self.visit_expression(expr)?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(Some(Statement::Return { value, meta }))
            }
            "if_statement" => {
                let cond_node = node
                    .child_by_field_name("condition")
                    .unwrap()
                    .child(1)
                    .unwrap(); // inside parens
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
            "compound_statement" => {
                // Handled by visit_block
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
        let meta = self.base.create_meta(node, "c", self.file_name);

        match node.kind() {
            "identifier" => Ok(Expression::Var {
                name: self.base.text(node)?.to_string(),
                meta,
            }),
            "number_literal" => {
                let text = self.base.text(node)?;
                if text.contains('.') {
                    Ok(Expression::FloatLiteral {
                        value: self.base.extract_float_literal(node)?,
                        meta,
                    })
                } else {
                    Ok(Expression::IntLiteral {
                        value: self.base.extract_int_literal(node)?,
                        meta,
                    })
                }
            }
            "string_literal" => Ok(Expression::StringLiteral {
                value: self.base.extract_string_literal(node)?,
                meta,
            }),
            "true" => Ok(Expression::BoolLiteral { value: true, meta }),
            "false" => Ok(Expression::BoolLiteral { value: false, meta }),
            "null" => Ok(Expression::NullLiteral { meta }),
            "binary_expression" => {
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
            "call_expression" => {
                let func_node = node.child_by_field_name("function").unwrap();
                let args_node = node.child_by_field_name("arguments").unwrap();
                let mut args = Vec::new();
                for arg in args_node.children(&mut args_node.walk()) {
                    if arg.kind() != "(" && arg.kind() != ")" && arg.kind() != "," {
                        args.push(self.visit_expression(arg)?);
                    }
                }

                let func_name = self.base.text(func_node)?;

                // Use centralized capability mapping
                if let Some(cap_name) = walker_core::map_to_capability("c", func_name) {
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
            }
            _ => {
                if node.child_count() == 1 {
                    self.visit_expression(node.child(0).unwrap())
                } else {
                    Ok(Expression::NullLiteral { meta })
                }
            }
        }
    }

    fn extract_name_from_declarator(&self, node: Node) -> Result<&str> {
        if node.kind() == "identifier" {
            self.base.text(node)
        } else if let Some(func_node) = node.child_by_field_name("declarator") {
            self.extract_name_from_declarator(func_node)
        } else {
            self.base.text(node)
        }
    }
}
