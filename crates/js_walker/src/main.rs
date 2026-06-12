use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use crush_cast::{self as ast, CastType, Expression, Statement};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tree_sitter::{Node, Parser, Tree};
use walker_core::{BaseWalker, Walker};

#[derive(ClapParser)]
#[command(name = "js_walker")]
struct Cli {
    /// Input JavaScript/TypeScript source file
    input: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let source_code = fs::read_to_string(&cli.input)?;

    let mut parser = Parser::new();
    let language: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();
    parser
        .set_language(&language)
        .context("Error loading JavaScript grammar")?;

    let tree = parser
        .parse(&source_code, None)
        .context("Error parsing source code")?;

    let walker = JsWalker {
        file_name: cli.input.to_string(),
    };

    let program = walker.walk(&tree, source_code.as_bytes())?;

    println!("{}", serde_json::to_string_pretty(&program)?);

    Ok(())
}

struct JsWalker {
    file_name: String,
}

impl Walker for JsWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_javascript::LANGUAGE.into()
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
            lang: Some("javascript".to_string()),
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
        let meta = self.base.create_meta(node, "javascript", self.file_name);
        match node.kind() {
            "import_statement" | "import_declaration" => {
                // Collect import statements
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "string" {
                        if let Ok(import_path) = self.base.extract_string_literal(child) {
                            self.imports.push(import_path);
                        }
                    }
                }
                Ok(None)
            }
            "lexical_declaration" | "variable_declaration" => {
                // Handle let, const, var declarations
                let mut cursor = node.walk();
                let mut declarations = node.children_by_field_name("declarator", &mut cursor);
                if let Some(declarator) = declarations.next() {
                    if let Some(name_node) = declarator.child_by_field_name("name") {
                        let name = self.base.text(name_node)?.to_string();
                        let value = if let Some(init_node) = declarator.child_by_field_name("value")
                        {
                            self.visit_expression(init_node)?
                        } else {
                            Expression::NullLiteral { meta: meta.clone() }
                        };
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
            "function_declaration" => {
                let name = if let Some(name_node) = node.child_by_field_name("name") {
                    self.base.text(name_node)?.to_string()
                } else {
                    "anonymous".to_string()
                };
                let params = self.visit_parameters(node)?;
                let body_node = node.child_by_field_name("body").unwrap();
                let body = self.visit_block(body_node)?;
                self.functions.insert(
                    name.clone(),
                    ast::Function {
                        params: params.clone(),
                        body,
                        meta: meta.clone(),
                    },
                );
                Ok(None)
            }
            "arrow_function" => {
                // Arrow functions - treat as anonymous functions for now
                let params = self.visit_parameters(node)?;
                let body = if let Some(body_node) = node.child_by_field_name("body") {
                    if body_node.kind() == "statement_block" {
                        self.visit_block(body_node)?
                    } else {
                        // Expression body - wrap in return
                        vec![Statement::Return {
                            value: Some(self.visit_expression(body_node)?),
                            meta: meta.clone(),
                        }]
                    }
                } else {
                    vec![]
                };
                // Create anonymous function name
                let func_name = format!("_arrow_{}", self.functions.len());
                self.functions.insert(
                    func_name.clone(),
                    ast::Function {
                        params,
                        body,
                        meta: meta.clone(),
                    },
                );
                Ok(None)
            }
            "expression_statement" => {
                let child = node.child(0).unwrap();
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
                let condition = if let Some(cond_node) = node.child_by_field_name("condition") {
                    self.visit_expression(cond_node)?
                } else {
                    return Ok(None);
                };
                let then_body = if let Some(then_node) = node.child_by_field_name("consequence") {
                    if then_node.kind() == "statement_block" {
                        self.visit_block(then_node)?
                    } else {
                        vec![self
                            .visit_statement(then_node)?
                            .unwrap_or_else(|| Statement::Break { meta: meta.clone() })]
                    }
                } else {
                    vec![]
                };
                let else_body = if let Some(else_node) = node.child_by_field_name("alternative") {
                    Some(if else_node.kind() == "statement_block" {
                        self.visit_block(else_node)?
                    } else {
                        vec![self
                            .visit_statement(else_node)?
                            .unwrap_or_else(|| Statement::Break { meta: meta.clone() })]
                    })
                } else {
                    None
                };
                Ok(Some(Statement::If {
                    condition,
                    then_body,
                    else_body,
                    meta,
                }))
            }
            "while_statement" => {
                let condition = if let Some(cond_node) = node.child_by_field_name("condition") {
                    Box::new(self.visit_expression(cond_node)?)
                } else {
                    return Ok(None);
                };
                let body = if let Some(body_node) = node.child_by_field_name("body") {
                    if body_node.kind() == "statement_block" {
                        self.visit_block(body_node)?
                    } else {
                        vec![self
                            .visit_statement(body_node)?
                            .unwrap_or_else(|| Statement::Break { meta: meta.clone() })]
                    }
                } else {
                    vec![]
                };
                Ok(Some(Statement::While {
                    condition,
                    body,
                    meta,
                }))
            }
            "for_statement" => {
                // Handle for loops: for (init; condition; update) body
                let body = if let Some(body_node) = node.child_by_field_name("body") {
                    if body_node.kind() == "statement_block" {
                        self.visit_block(body_node)?
                    } else {
                        vec![self
                            .visit_statement(body_node)?
                            .unwrap_or_else(|| Statement::Break { meta: meta.clone() })]
                    }
                } else {
                    vec![]
                };
                // For now, convert to while loop structure
                // Extract variable and iterable if it's a for...of loop
                if let Some(init_node) = node.child_by_field_name("initializer") {
                    if init_node.kind() == "lexical_declaration"
                        || init_node.kind() == "variable_declaration"
                    {
                        let mut cursor = init_node.walk();
                        let declarators: Vec<_> = init_node
                            .children_by_field_name("declarator", &mut cursor)
                            .collect();
                        if let Some(declarator) = declarators.first() {
                            if let Some(name_node) = declarator.child_by_field_name("name") {
                                let variable = self.base.text(name_node)?.to_string();
                                if let Some(value_node) = declarator.child_by_field_name("value") {
                                    let iterable = Box::new(self.visit_expression(value_node)?);
                                    return Ok(Some(Statement::For {
                                        variable,
                                        iterable,
                                        body,
                                        meta,
                                    }));
                                }
                            }
                        }
                    }
                }
                // Fallback: treat as while loop
                let condition = if let Some(cond_node) = node.child_by_field_name("condition") {
                    Box::new(self.visit_expression(cond_node)?)
                } else {
                    Box::new(Expression::BoolLiteral {
                        value: true,
                        meta: meta.clone(),
                    })
                };
                Ok(Some(Statement::While {
                    condition,
                    body,
                    meta,
                }))
            }
            "return_statement" => {
                let value = if let Some(val_node) = node.child_by_field_name("value") {
                    Some(self.visit_expression(val_node)?)
                } else {
                    None
                };
                Ok(Some(Statement::Return { value, meta }))
            }
            "break_statement" => Ok(Some(Statement::Break { meta })),
            "continue_statement" => Ok(Some(Statement::Continue { meta })),
            "throw_statement" => {
                let value = if let Some(val_node) = node.child_by_field_name("value") {
                    self.visit_expression(val_node)?
                } else {
                    Expression::NullLiteral { meta: meta.clone() }
                };
                Ok(Some(Statement::Throw { value, meta }))
            }
            "try_statement" => {
                let body = if let Some(body_node) = node.child_by_field_name("body") {
                    if body_node.kind() == "statement_block" {
                        self.visit_block(body_node)?
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };
                let error_var = "error".to_string(); // Default error variable name
                let handler = if let Some(catch_node) = node.child_by_field_name("handler") {
                    if let Some(handler_body) = catch_node.child_by_field_name("body") {
                        if handler_body.kind() == "statement_block" {
                            self.visit_block(handler_body)?
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };
                Ok(Some(Statement::TryCatch {
                    body,
                    error_var,
                    handler,
                    meta,
                }))
            }
            "statement_block" => {
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

    fn visit_parameters(&mut self, node: Node) -> Result<Vec<(String, CastType)>> {
        let mut params = Vec::new();
        if let Some(params_node) = node.child_by_field_name("parameters") {
            for child in params_node.children(&mut params_node.walk()) {
                match child.kind() {
                    "identifier" | "property_identifier" => {
                        params.push((self.base.text(child)?.to_string(), CastType::Any));
                    }
                    "required_parameter" | "optional_parameter" => {
                        if let Some(name_node) = child.child_by_field_name("pattern") {
                            if let Some(id_node) = name_node.child(0) {
                                if id_node.kind() == "identifier"
                                    || id_node.kind() == "property_identifier"
                                {
                                    params.push((
                                        self.base.text(id_node)?.to_string(),
                                        CastType::Any,
                                    ));
                                }
                            }
                        } else if let Some(id_node) = child.child(0) {
                            if id_node.kind() == "identifier"
                                || id_node.kind() == "property_identifier"
                            {
                                params.push((self.base.text(id_node)?.to_string(), CastType::Any));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(params)
    }

    fn visit_expression(&mut self, node: Node) -> Result<Expression> {
        let node = self.base.unwrap_parens(node);
        let meta = self.base.create_meta(node, "javascript", self.file_name);

        match node.kind() {
            "identifier" | "property_identifier" => Ok(Expression::Var {
                name: self.base.text(node)?.to_string(),
                meta,
            }),
            "number" => {
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
            "string" | "template_string" => Ok(Expression::StringLiteral {
                value: self.base.extract_string_literal(node)?,
                meta,
            }),
            "true" => Ok(Expression::BoolLiteral { value: true, meta }),
            "false" => Ok(Expression::BoolLiteral { value: false, meta }),
            "null" | "undefined" => Ok(Expression::NullLiteral { meta }),
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
            "unary_expression" => {
                let operator = self.base.text(node.child(0).unwrap())?.to_string();
                let operand = Box::new(self.visit_expression(node.child(1).unwrap())?);
                Ok(Expression::UnaryOp {
                    operator,
                    operand,
                    meta,
                })
            }
            "call_expression" => {
                let func_node = node.child_by_field_name("function").unwrap();
                let args_node = node.child_by_field_name("arguments").unwrap();
                let mut args = Vec::new();
                for arg in args_node.children(&mut args_node.walk()) {
                    if arg.kind() == "expression"
                        || arg.kind() == "identifier"
                        || arg.kind() == "number"
                        || arg.kind() == "string"
                    {
                        args.push(self.visit_expression(arg)?);
                    }
                }

                if func_node.kind() == "identifier" || func_node.kind() == "property_identifier" {
                    let func_name = self.base.text(func_node)?;

                    // Use centralized capability mapping
                    if let Some(cap_name) = walker_core::map_to_capability("javascript", func_name)
                    {
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
                    // Function call with complex expression
                    let func_expr = self.visit_expression(func_node)?;
                    if let Expression::Var { name, .. } = func_expr {
                        Ok(Expression::Call {
                            function: name,
                            args,
                            meta,
                        })
                    } else {
                        Ok(Expression::NullLiteral { meta })
                    }
                }
            }
            "member_expression" => {
                let object =
                    Box::new(self.visit_expression(node.child_by_field_name("object").unwrap())?);
                let property = if let Some(prop_node) = node.child_by_field_name("property") {
                    self.base.text(prop_node)?.to_string()
                } else {
                    "".to_string()
                };
                Ok(Expression::GetField {
                    target: object,
                    field: property,
                    meta,
                })
            }
            "assignment_expression" => {
                let _left = self.visit_expression(node.child_by_field_name("left").unwrap())?;
                let right = self.visit_expression(node.child_by_field_name("right").unwrap())?;
                // Assignment expressions in JS return the assigned value
                // Field setting is handled at the statement level via Statement::SetField
                Ok(right)
            }
            "await_expression" => {
                let expression = Box::new(
                    self.visit_expression(node.child_by_field_name("expression").unwrap())?,
                );
                Ok(Expression::Await { expression, meta })
            }
            "array" => {
                // Parse array elements
                let mut elements = Vec::new();
                for child in node.children(&mut node.walk()) {
                    // Skip punctuation
                    if !matches!(child.kind(), "[" | "]" | ",") {
                        elements.push(self.visit_expression(child)?);
                    }
                }
                Ok(Expression::ArrayLiteral { elements, meta })
            }
            "object" => {
                // Parse object properties
                let mut properties = Vec::new();
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "pair" || child.kind() == "property" {
                        // Get key and value
                        let key = child
                            .child_by_field_name("key")
                            .or_else(|| child.child(0))
                            .map(|k| {
                                self.base
                                    .text(k)
                                    .unwrap_or("")
                                    .trim_matches('"')
                                    .to_string()
                            })
                            .unwrap_or_default();
                        let value = child
                            .child_by_field_name("value")
                            .or_else(|| child.child(2))
                            .map(|v| self.visit_expression(v))
                            .transpose()?
                            .unwrap_or(Expression::NullLiteral { meta: meta.clone() });
                        properties.push((key, value));
                    }
                }
                Ok(Expression::ObjectLiteral { properties, meta })
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
