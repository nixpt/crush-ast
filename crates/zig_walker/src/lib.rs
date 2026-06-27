//! # Zig Walker
//!
//! Transforms Zig source code into CAST (Crush Abstract Syntax Tree).
//!
//! ## Classification
//! This is a **walker/transpiler**, not a runtime. It converts Zig source
//! to CAST which is then compiled to CASM and executed on the CRUSH VM.

use anyhow::Result;
use crush_cast::{self as ast, CastType, Expression, Statement};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};
use walker_core::{BaseWalker, Walker};

pub struct ZigWalker {
    pub file_name: String,
}

impl Walker for ZigWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_zig::LANGUAGE.into()
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
            main_body.extend(visitor.visit_statement(child)?);
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
            cast_version: "0.2".to_string(),
            entry: "main".to_string(),
            lang: Some("zig".to_string()),
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
    fn visit_statement(&mut self, node: Node) -> Result<Vec<Statement>> {
        let meta = self.base.create_meta(node, "zig", self.file_name);
        match node.kind() {
            "FnDecl" | "fn_decl" | "function_declaration" => {
                self.visit_function(node)?;
                Ok(vec![])
            }
            "VarDecl" | "var_decl" | "const_decl" | "variable_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .or_else(|| {
                        node.children(&mut node.walk())
                            .find(|c| c.kind() == "identifier" || c.kind() == "Identifier")
                    })
                    .map(|n| self.base.text(n).unwrap_or("_").to_string())
                    .unwrap_or_else(|| "_".to_string());

                let value = node
                    .child_by_field_name("value")
                    .or_else(|| {
                        node.children(&mut node.walk())
                            .filter(|c| c.kind() != "identifier" && c.kind() != "const" && c.kind() != "var" && c.kind() != "=" && c.kind() != ";")
                            .last()
                    })
                    .map(|n| self.visit_expression(n))
                    .transpose()?
                    .unwrap_or(Expression::NullLiteral { meta: meta.clone() });

                Ok(vec![Statement::VarDecl {
                    name,
                    value,
                    type_hint: CastType::Any,
                    meta,
                }])
            }
            "ReturnStatement" | "return_statement" => {
                let value = node
                    .child_by_field_name("value")
                    .map(|n| self.visit_expression(n))
                    .transpose()?;
                Ok(vec![Statement::Return { value, meta }])
            }
            "BreakStatement" | "break_statement" => Ok(vec![Statement::Break { meta }]),
            "ContinueStatement" | "continue_statement" => Ok(vec![Statement::Continue { meta }]),
            "WhileExpr" | "while_expression" | "while_statement" => {
                let condition_node = node.child_by_field_name("condition")
                    .or_else(|| node.child_by_field_name("test"))
                    .unwrap_or_else(|| node.child(1).unwrap());
                let condition = self.visit_expression(condition_node)?;
                
                let body_node = node.child_by_field_name("body")
                    .unwrap_or_else(|| node.child(node.child_count() - 1).unwrap());
                let body = self.visit_block_or_statement(body_node)?;
                
                Ok(vec![Statement::While {
                    condition: Box::new(condition),
                    body,
                    meta,
                }])
            }
            "ForExpr" | "for_expression" | "for_statement" => {
                let iterable_node = node.child_by_field_name("iterable")
                    .or_else(|| node.child_by_field_name("inputs"))
                    .unwrap_or_else(|| node.child(1).unwrap());
                let iterable = self.visit_expression(iterable_node)?;
                
                let variable = extract_payload_variable(node, self.base.source)
                    .unwrap_or_else(|| "item".to_string());
                
                let body_node = node.child_by_field_name("body")
                    .unwrap_or_else(|| node.child(node.child_count() - 1).unwrap());
                let body = self.visit_block_or_statement(body_node)?;
                
                Ok(vec![Statement::For {
                    variable,
                    iterable: Box::new(iterable),
                    body,
                    meta,
                }])
            }
            "Block" | "block" => {
                self.visit_block(node)
            }
            _ if node.kind() == "AssignmentExpr" || node.kind() == "assignment_expression" => {
                let left_node = node.child_by_field_name("left")
                    .unwrap_or_else(|| node.child(0).unwrap());
                let right_node = node.child_by_field_name("right")
                    .unwrap_or_else(|| node.child(2).unwrap());
                let right_expr = self.visit_expression(right_node)?;
                let op_str = self.base.text(node.child(1).unwrap())?.to_string();
                
                if left_node.kind() == "MemberAccessExpr" || left_node.kind() == "member_access_expression" || left_node.kind() == "field_expression" {
                    let obj_node = left_node.child_by_field_name("object")
                        .or_else(|| left_node.child(0))
                        .unwrap();
                    let target = self.visit_expression(obj_node)?;
                    let field = self.base.text(left_node.child_by_field_name("member").or_else(|| left_node.child(2)).unwrap())?.to_string();
                    
                    let value = match op_str.as_str() {
                        "=" => right_expr,
                        _ => Expression::BinaryOp {
                            operator: op_str.trim_end_matches('=').to_string(),
                            left: Box::new(Expression::GetField {
                                target: Box::new(target.clone()),
                                field: field.clone(),
                                meta: meta.clone(),
                            }),
                            right: Box::new(right_expr),
                            meta: meta.clone(),
                        }
                    };
                    
                    return Ok(vec![Statement::SetField {
                        target,
                        field,
                        value,
                        meta,
                    }]);
                }
                
                let expr = self.visit_expression(node)?;
                Ok(vec![Statement::ExprStmt { expr, meta }])
            }
            _ => {
                if let Ok(expr) = self.visit_expression(node) {
                    Ok(vec![Statement::ExprStmt { expr, meta }])
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    fn visit_block_or_statement(&mut self, node: Node) -> Result<Vec<Statement>> {
        if node.kind() == "Block" || node.kind() == "block" {
            self.visit_block(node)
        } else {
            self.visit_statement(node)
        }
    }

    fn visit_block(&mut self, node: Node) -> Result<Vec<Statement>> {
        let mut body = Vec::new();
        for child in node.children(&mut node.walk()) {
            if child.kind() != "{" && child.kind() != "}" {
                body.extend(self.visit_statement(child)?);
            }
        }
        Ok(body)
    }

    fn visit_function(&mut self, node: Node) -> Result<()> {
        let meta = self.base.create_meta(node, "zig", self.file_name);

        let name = node
            .child_by_field_name("name")
            .or_else(|| {
                node.children(&mut node.walk())
                    .find(|c| c.kind() == "identifier" || c.kind() == "Identifier")
            })
            .map(|n| self.base.text(n).unwrap_or("anonymous").to_string())
            .unwrap_or_else(|| "anonymous".to_string());

        let params = self.visit_params(node)?;
        let body = node
            .child_by_field_name("body")
            .map(|b| self.visit_block(b))
            .transpose()?
            .unwrap_or_default();

        self.functions.insert(
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

    fn visit_params(&mut self, node: Node) -> Result<Vec<(String, CastType)>> {
        let mut params = Vec::new();
        if let Some(params_node) = node.child_by_field_name("params") {
            for child in params_node.children(&mut params_node.walk()) {
                if matches!(child.kind(), "param" | "Param" | "parameter") {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        params.push((self.base.text(name_node)?.to_string(), CastType::Any));
                    }
                }
            }
        }
        Ok(params)
    }

    fn visit_expression(&mut self, node: Node) -> Result<Expression> {
        let node = self.base.unwrap_parens(node);
        let meta = self.base.create_meta(node, "zig", self.file_name);

        match node.kind() {
            "Identifier" | "identifier" => Ok(Expression::Var {
                name: self.base.text(node)?.to_string(),
                meta,
            }),
            "IntegerLiteral" | "integer_literal" => Ok(Expression::IntLiteral {
                value: self.base.text(node)?.parse().unwrap_or(0),
                meta,
            }),
            "StringLiteral" | "string_literal" => Ok(Expression::StringLiteral {
                value: self.base.text(node)?.trim_matches('"').to_string(),
                meta,
            }),
            "true" => Ok(Expression::BoolLiteral { value: true, meta }),
            "false" => Ok(Expression::BoolLiteral { value: false, meta }),
            "null" => Ok(Expression::NullLiteral { meta }),
            "BinaryExpr" | "binary_expr" if node.child_count() >= 3 => {
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
            "AssignmentExpr" | "assignment_expression" => {
                let left_node = node.child_by_field_name("left")
                    .unwrap_or_else(|| node.child(0).unwrap());
                let right_node = node.child_by_field_name("right")
                    .unwrap_or_else(|| node.child(2).unwrap());
                
                let name = self.base.text(left_node)?.to_string();
                let right_expr = self.visit_expression(right_node)?;
                let op_str = self.base.text(node.child(1).unwrap())?;
                
                let value = match op_str {
                    "=" => right_expr,
                    _ => Expression::BinaryOp {
                        operator: op_str.trim_end_matches('=').to_string(),
                        left: Box::new(Expression::Var { name: name.clone(), meta: meta.clone() }),
                        right: Box::new(right_expr),
                        meta: meta.clone(),
                    }
                };
                
                Ok(Expression::Call {
                    function: "__crush_assign__".to_string(),
                    args: vec![
                        Expression::Var { name, meta: meta.clone() },
                        value,
                    ],
                    meta,
                })
            }
            "MemberAccessExpr" | "member_access_expression" | "field_expression" => {
                let obj_node = node.child_by_field_name("object")
                    .or_else(|| node.child(0))
                    .unwrap();
                let target = self.visit_expression(obj_node)?;
                let field = self.base.text(node.child_by_field_name("member").or_else(|| node.child(2)).unwrap())?.to_string();
                Ok(Expression::GetField {
                    target: Box::new(target),
                    field,
                    meta,
                })
            }
            "CallExpr" | "call_expr" => {
                let func_node = node
                    .child_by_field_name("function")
                    .or(node.child(0))
                    .unwrap();
                let func_name = self.base.text(func_node)?.to_string();

                let mut args = Vec::new();
                if let Some(args_node) = node.child_by_field_name("args") {
                    for arg in args_node.children(&mut args_node.walk()) {
                        if !matches!(arg.kind(), "(" | ")" | ",") {
                            if let Ok(expr) = self.visit_expression(arg) {
                                args.push(expr);
                            }
                        }
                    }
                }

                // Centralized capability mapping
                if let Some(cap_name) = walker_core::map_to_capability("zig", &func_name) {
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

                Ok(Expression::Call {
                    function: if func_name == "std.debug.print" { "print".to_string() } else { func_name.clone() },
                    args,
                    meta,
                })
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
}

fn extract_payload_variable(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "payload" || child.kind() == "Payload" {
            let mut sub_cursor = child.walk();
            for sub_child in child.children(&mut sub_cursor) {
                if sub_child.kind() == "identifier" || sub_child.kind() == "Identifier" {
                    if let Ok(text) = std::str::from_utf8(&source[sub_child.byte_range()]) {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}


#[cfg(test)]
mod tests {
    use crate::*;
    use tree_sitter::Parser;

    fn parse_and_walk(source: &str) -> ast::Program {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_zig::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        println!("AST S-expression: {}", tree.root_node().to_sexp());
        let walker = ZigWalker {
            file_name: "test.zig".to_string(),
        };
        walker.walk(&tree, source.as_bytes()).unwrap()
    }

    #[test]
    fn test_variable_declaration() {
        let program = parse_and_walk("const x = 42;");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::VarDecl { name, value, .. } = &main_func.body[0] {
            assert_eq!(name, "x");
            if let Expression::IntLiteral { value: val, .. } = value {
                assert_eq!(*val, 42);
            }
        }
    }

    #[test]
    fn test_while_loop() {
        let program = parse_and_walk("fn main() void { while (x > 0) { x = x - 1; } }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::While { condition, body, .. } = &main_func.body[0] {
            if let Expression::BinaryOp { operator, .. } = &**condition {
                assert_eq!(operator, ">");
            }
            assert_eq!(body.len(), 1);
        }
    }

    #[test]
    fn test_for_loop() {
        let program = parse_and_walk("fn main() void { for (items) |item| { std.debug.print(item); } }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::For { variable, iterable, body, .. } = &main_func.body[0] {
            assert_eq!(variable, "item");
            if let Expression::Var { name, .. } = &**iterable {
                assert_eq!(name, "items");
            }
            assert_eq!(body.len(), 1);
            if let Statement::ExprStmt { expr, .. } = &body[0] {
                if let Expression::CapabilityCall { name, .. } = expr {
                    assert_eq!(name, "io.print");
                }
            }
        }
    }

    #[test]
    fn test_member_assignment() {
        let program = parse_and_walk("fn main() void { obj.x = 10; }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::SetField { target, field, value, .. } = &main_func.body[0] {
            assert_eq!(field, "x");
            if let Expression::Var { name, .. } = target {
                assert_eq!(name, "obj");
            }
            if let Expression::IntLiteral { value: val, .. } = value {
                assert_eq!(*val, 10);
            }
        }
    }
}
