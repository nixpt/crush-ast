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
    fn visit_statement(&mut self, node: Node) -> Result<Vec<Statement>> {
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
                Ok(vec![])
            }
            "declaration" => {
                let mut decls = Vec::new();
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "init_declarator" {
                        let name_node = child.child_by_field_name("declarator").unwrap();
                        let name = self.base.text(name_node)?.to_string();
                        let value_node = child.child_by_field_name("value").unwrap();
                        let value = self.visit_expression(value_node)?;
                        decls.push(Statement::VarDecl {
                            name,
                            value,
                            type_hint: CastType::Any,
                            meta: meta.clone(),
                        });
                    }
                }
                if decls.is_empty() {
                    if let Some(decl) = node.child_by_field_name("declarator") {
                        let name = self.extract_name_from_declarator(decl)?;
                        decls.push(Statement::VarDecl {
                            name: name.to_string(),
                            value: Expression::NullLiteral { meta: meta.clone() },
                            type_hint: CastType::Any,
                            meta: meta.clone(),
                        });
                    }
                }
                Ok(decls)
            }
            "expression_statement" => {
                let expr_node = node.child(0).unwrap();
                if expr_node.kind() == "assignment_expression" {
                    let left_node = expr_node.child_by_field_name("left").unwrap();
                    let op_str = self.base.text(expr_node.child_by_field_name("operator").unwrap())?.to_string();
                    let right_node = expr_node.child_by_field_name("right").unwrap();
                    let right_expr = self.visit_expression(right_node)?;
                    
                    if left_node.kind() == "field_expression" {
                        let target_node = left_node.child_by_field_name("argument").unwrap();
                        let target = self.visit_expression(target_node)?;
                        let field = self.base.text(left_node.child_by_field_name("field").unwrap())?.to_string();
                        
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
                }
                
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
                            return Ok(vec![Statement::Export {
                                name: export_name.clone(),
                                value: args[1].clone(),
                                meta,
                            }]);
                        }
                    }
                }

                Ok(vec![Statement::ExprStmt { expr, meta }])
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
                Ok(vec![Statement::Return { value, meta }])
            }
            "if_statement" => {
                let cond_node = node
                    .child_by_field_name("condition")
                    .unwrap()
                    .child(1)
                    .unwrap(); // inside parens
                let condition = self.visit_expression(cond_node)?;
                let cons_node = node.child_by_field_name("consequence").unwrap();
                let then_body = self.visit_block_or_statement(cons_node)?;

                let mut else_body = None;
                if let Some(alt_node) = node.child_by_field_name("alternative") {
                    else_body = Some(self.visit_block_or_statement(alt_node)?);
                }
                Ok(vec![Statement::If {
                    condition,
                    then_body,
                    else_body,
                    meta,
                }])
            }
            "while_statement" => {
                let cond_node = node
                    .child_by_field_name("condition")
                    .unwrap()
                    .child(1)
                    .unwrap(); // inside parens
                let condition = self.visit_expression(cond_node)?;
                let body_node = node.child_by_field_name("body").unwrap();
                let body = self.visit_block_or_statement(body_node)?;
                Ok(vec![Statement::While {
                    condition: Box::new(condition),
                    body,
                    meta,
                }])
            }
            "for_statement" => {
                let mut for_statements = Vec::new();
                
                // 1. Initializer
                if let Some(init_node) = node.child_by_field_name("initializer") {
                    for_statements.extend(self.visit_statement(init_node)?);
                }
                
                // 2. Condition
                let condition = if let Some(cond_node) = node.child_by_field_name("condition") {
                    self.visit_expression(cond_node)?
                } else {
                    Expression::BoolLiteral { value: true, meta: meta.clone() }
                };
                
                // 3. Body
                let body_node = node.child_by_field_name("body").unwrap();
                let mut while_body = self.visit_block_or_statement(body_node)?;
                
                // 4. Update
                if let Some(update_node) = node.child_by_field_name("update") {
                    let update_expr = self.visit_expression(update_node)?;
                    while_body.push(Statement::ExprStmt {
                        expr: update_expr,
                        meta: self.base.create_meta(update_node, "c", self.file_name),
                    });
                }
                
                for_statements.push(Statement::While {
                    condition: Box::new(condition),
                    body: while_body,
                    meta,
                });
                
                Ok(for_statements)
            }
            "compound_statement" => {
                self.visit_block(node)
            }
            _ => Ok(vec![]),
        }
    }

    fn visit_block_or_statement(&mut self, node: Node) -> Result<Vec<Statement>> {
        if node.kind() == "compound_statement" {
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

    fn visit_expression(&mut self, node: Node) -> Result<Expression> {
        let node = self.base.unwrap_parens(node);
        let meta = self.base.create_meta(node, "c", self.file_name);

        match node.kind() {
            "identifier" => Ok(Expression::Var {
                name: self.base.text(node)?.to_string(),
                meta,
            }),
            "field_expression" => {
                let target_node = node.child_by_field_name("argument").unwrap();
                let target = self.visit_expression(target_node)?;
                let field = self.base.text(node.child_by_field_name("field").unwrap())?.to_string();
                Ok(Expression::GetField {
                    target: Box::new(target),
                    field,
                    meta,
                })
            }
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
            "assignment_expression" => {
                let left_node = node.child_by_field_name("left").unwrap();
                let right_node = node.child_by_field_name("right").unwrap();
                let name = self.base.text(left_node)?.to_string();
                let right_expr = self.visit_expression(right_node)?;
                let op_str = self.base.text(node.child_by_field_name("operator").unwrap())?;
                
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
            "update_expression" => {
                let arg_node = node.child_by_field_name("argument").unwrap();
                let name = self.base.text(arg_node)?.to_string();
                let op = self.base.text(node.child(0).unwrap())?;
                let is_prefix = op == "++" || op == "--";
                let op_str = if is_prefix { op } else { self.base.text(node.child(1).unwrap())? };
                
                let fname = match (op_str, is_prefix) {
                    ("++", true) => "__crush_pre_inc__",
                    ("--", true) => "__crush_pre_dec__",
                    ("++", false) => "__crush_post_inc__",
                    ("--", false) => "__crush_post_dec__",
                    _ => "__crush_post_inc__",
                };
                
                Ok(Expression::Call {
                    function: fname.to_string(),
                    args: vec![Expression::Var {
                        name,
                        meta: meta.clone(),
                    }],
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_walk(source: &str) -> ast::Program {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let walker = CWalker {
            file_name: "test.c".to_string(),
        };
        walker.walk(&tree, source.as_bytes()).unwrap()
    }

    #[test]
    fn test_variable_declaration() {
        let program = parse_and_walk("void main() { int x = 42; }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::VarDecl { name, value, .. } = &main_func.body[0] {
            assert_eq!(name, "x");
            if let Expression::IntLiteral { value: val, .. } = value {
                assert_eq!(*val, 42);
            } else {
                panic!("Expected IntLiteral");
            }
        } else {
            panic!("Expected VarDecl");
        }
    }

    #[test]
    fn test_while_loop() {
        let program = parse_and_walk("void main() { while (x > 0) { x = x - 1; } }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::While { condition, body, .. } = &main_func.body[0] {
            if let Expression::BinaryOp { operator, .. } = &**condition {
                assert_eq!(operator, ">");
            } else {
                panic!("Expected BinaryOp condition");
            }
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected While statement");
        }
    }

    #[test]
    fn test_for_loop_desugaring() {
        let program = parse_and_walk("void main() { for (int i = 0; i < 10; i++) { printf(i); } }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 2);
        
        if let Statement::VarDecl { name, .. } = &main_func.body[0] {
            assert_eq!(name, "i");
        } else {
            panic!("Expected VarDecl initializer");
        }

        if let Statement::While { condition, body, .. } = &main_func.body[1] {
            if let Expression::BinaryOp { operator, .. } = &**condition {
                assert_eq!(operator, "<");
            }
            assert_eq!(body.len(), 2);
            if let Statement::ExprStmt { expr, .. } = &body[1] {
                if let Expression::Call { function, .. } = expr {
                    assert_eq!(function, "__crush_post_inc__");
                } else {
                    panic!("Expected update call");
                }
            } else {
                panic!("Expected ExprStmt for update");
            }
        } else {
            panic!("Expected While statement");
        }
    }

    #[test]
    fn test_member_assignment() {
        let program = parse_and_walk("void main() { obj.x = 10; }");
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
        } else {
            panic!("Expected SetField statement");
        }
    }

    #[test]
    fn test_capability_call_mapping() {
        let program = parse_and_walk("void main() { printf(\"hello\"); }");
        let main_func = program.functions.get("main").unwrap();
        assert_eq!(main_func.body.len(), 1);
        if let Statement::ExprStmt { expr, .. } = &main_func.body[0] {
            if let Expression::CapabilityCall { name, .. } = expr {
                assert_eq!(name, "io.print");
            } else {
                panic!("Expected CapabilityCall");
            }
        } else {
            panic!("Expected ExprStmt");
        }
    }
}
