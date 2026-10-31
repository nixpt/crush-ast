use anyhow::Result;
use crush_cast::{self as ast, CastType, Expression, Statement};
use crush_walker_core::{BaseWalker, Walker};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

pub struct GoWalker {
    pub file_name: String,
}

impl Walker for GoWalker {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
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
            main_body.extend(visitor.visit_statements(child)?);
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
            lang: Some("go".to_string()),
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
    /// The statements a node lowers to.
    ///
    /// `function_declaration` hoists into `functions` and yields none; a bare
    /// `block` yields its inner statements; a C-style `for` yields its
    /// initializer plus a `while`; `if`/`for` with an initializer yield it
    /// first. Every statement kind this walker does not support is a loud
    /// error — never a silent drop (CRUSH-198).
    fn visit_statements(&mut self, node: Node) -> Result<Vec<Statement>> {
        let meta = self.base.create_meta(node, "go", self.file_name);
        match node.kind() {
            "function_declaration" => {
                let name = self.base.child_text(node, "name")?.to_string();
                let params_node = node.child_by_field_name("parameters").unwrap();
                let mut params = Vec::new();
                for p_node in params_node.children(&mut params_node.walk()) {
                    if p_node.kind() == "parameter_declaration" {
                        let p_name = self.base.child_text(p_node, "name")?;
                        params.push((p_name.to_string(), CastType::Any));
                    }
                }
                let body_node = node.child_by_field_name("body").unwrap();
                let body = self.visit_block(body_node)?;
                self.functions.insert(
                    name,
                    ast::Function { params, body, meta, ..Default::default() },
                );
                Ok(vec![])
            }
            "short_var_declaration" => {
                // tree-sitter-go calls this `short_var_declaration`; the walker
                // matched `short_variable_declaration`, so `a := 7` was dropped
                // and `a` then read as uninitialised (CRUSH-198).
                let left = node.child_by_field_name("left").unwrap();
                let right = node.child_by_field_name("right").unwrap();
                let names = self.names_in(left)?;
                let values = self.values_in(right)?;
                if names.len() != values.len() {
                    anyhow::bail!(
                        "go: `:=` with {} name(s) and {} value(s) is not supported",
                        names.len(),
                        values.len()
                    );
                }
                Ok(names
                    .into_iter()
                    .zip(values)
                    .map(|(name, value)| Statement::VarDecl {
                        name,
                        value,
                        type_hint: CastType::Any,
                        meta: meta.clone(),
                    })
                    .collect())
            }
            "var_declaration" => {
                let mut out = Vec::new();
                for spec in node.children(&mut node.walk()) {
                    if spec.kind() != "var_spec" {
                        continue;
                    }
                    let name = self.base.child_text(spec, "name")?.to_string();
                    let value = match spec.child_by_field_name("value") {
                        Some(list) => self
                            .values_in(list)?
                            .into_iter()
                            .next()
                            .ok_or_else(|| anyhow::anyhow!("go: `var {name}` with no value"))?,
                        None => Expression::NullLiteral { meta: meta.clone() },
                    };
                    out.push(Statement::VarDecl {
                        name,
                        value,
                        type_hint: CastType::Any,
                        meta: meta.clone(),
                    });
                }
                Ok(out)
            }
            "assignment_statement" => {
                let left = node.child_by_field_name("left").unwrap();
                let right = node.child_by_field_name("right").unwrap();
                let names = self.names_in(left)?;
                let values = self.values_in(right)?;
                if names.len() != values.len() {
                    anyhow::bail!(
                        "go: assignment with {} target(s) and {} value(s) is not supported",
                        names.len(),
                        values.len()
                    );
                }
                Ok(names
                    .into_iter()
                    .zip(values)
                    .map(|(target, value)| Statement::Assign { target, value, meta: meta.clone() })
                    .collect())
            }
            "inc_statement" | "dec_statement" => {
                let target = self
                    .base
                    .text(node.named_child(0).ok_or_else(|| {
                        anyhow::anyhow!("go: malformed `{}`", node.kind())
                    })?)?
                    .to_string();
                let operator = if node.kind() == "inc_statement" { "+" } else { "-" };
                let value = Expression::BinaryOp {
                    operator: operator.to_string(),
                    left: Box::new(Expression::Var { name: target.clone(), meta: meta.clone() }),
                    right: Box::new(Expression::IntLiteral { value: 1, meta: meta.clone() }),
                    meta: meta.clone(),
                };
                Ok(vec![Statement::Assign { target, value, meta }])
            }
            "expression_statement" => {
                let expr_node = node.child(0).unwrap();
                let expr = self.visit_expression(expr_node)?;
                if let Expression::Call { ref function, ref args, .. } = expr {
                    if function == "__crush_export__" && args.len() == 2 {
                        if let Expression::StringLiteral { value: export_name, .. } = &args[0] {
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
                let value = match node.named_child(0) {
                    Some(child) if child.kind() == "expression_list" => {
                        let exprs = self.values_in(child)?;
                        if exprs.len() != 1 {
                            anyhow::bail!(
                                "go: `return` of {} values is not supported",
                                exprs.len()
                            );
                        }
                        exprs.into_iter().next()
                    }
                    Some(child) => Some(self.visit_expression(child)?),
                    None => None,
                };
                Ok(vec![Statement::Return { value, meta }])
            }
            "if_statement" => {
                let mut out = Vec::new();
                if let Some(init) = node.child_by_field_name("initializer") {
                    out.extend(self.visit_statements(init)?);
                }
                let cond_node = node.child_by_field_name("condition").unwrap();
                let condition = self.visit_expression(cond_node)?;
                let cons_node = node.child_by_field_name("consequence").unwrap();
                let then_body = self.visit_block(cons_node)?;
                let else_body = match node.child_by_field_name("alternative") {
                    Some(alt) => Some(self.visit_block(alt)?),
                    None => None,
                };
                out.push(Statement::If { condition, then_body, else_body, meta });
                Ok(out)
            }
            "for_statement" => self.visit_for(node, meta),
            "block" => self.visit_block(node),
            // Structural / trivia nodes that produce no Crush statement.
            "package_clause" | "import_declaration" | "comment" | ";" | "{" | "}" => Ok(vec![]),
            other => anyhow::bail!(
                "go: `{other}` is not supported by the Crush Go walker (it would otherwise be silently dropped)"
            ),
        }
    }

    /// `for` in all its Go shapes: `for {}`, `for cond {}`, the C-style
    /// `for init; cond; update {}` (lowered to `init` then
    /// `while cond { …; update }`) and `for x := range xs {}`.
    fn visit_for(
        &mut self,
        node: Node,
        meta: HashMap<String, serde_json::Value>,
    ) -> Result<Vec<Statement>> {
        let body_node = node
            .child_by_field_name("body")
            .ok_or_else(|| anyhow::anyhow!("go: `for` without a body"))?;
        let body = self.visit_block(body_node)?;

        if let Some(range) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "range_clause")
        {
            let left = range
                .child_by_field_name("left")
                .ok_or_else(|| anyhow::anyhow!("go: `range` without a target"))?;
            let names = self.names_in(left)?;
            if names.len() != 1 {
                anyhow::bail!(
                    "go: `for {} := range` (more than one variable) is not supported",
                    names.join(", ")
                );
            }
            let iterable = self.visit_expression(
                range
                    .child_by_field_name("right")
                    .ok_or_else(|| anyhow::anyhow!("go: `range` without an operand"))?,
            )?;
            return Ok(vec![Statement::For {
                variable: names.into_iter().next().unwrap(),
                iterable: Box::new(iterable),
                body,
                meta,
            }]);
        }

        if let Some(clause) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "for_clause")
        {
            let mut out = Vec::new();
            if let Some(init) = clause.child_by_field_name("initializer") {
                out.extend(self.visit_statements(init)?);
            }
            let condition = match clause.child_by_field_name("condition") {
                Some(c) => self.visit_expression(c)?,
                None => Expression::BoolLiteral {
                    value: true,
                    meta: meta.clone(),
                },
            };
            let mut loop_body = body;
            if let Some(update) = clause.child_by_field_name("update") {
                loop_body.extend(self.visit_statements(update)?);
            }
            out.push(Statement::While {
                condition: Box::new(condition),
                body: loop_body,
                meta,
            });
            return Ok(out);
        }

        // `for cond {}` or `for {}`: the condition, if present, is the first
        // named child that is not the body block.
        let condition = match node.named_child(0) {
            Some(c) if c.kind() != "block" => self.visit_expression(c)?,
            _ => Expression::BoolLiteral {
                value: true,
                meta: meta.clone(),
            },
        };
        Ok(vec![Statement::While {
            condition: Box::new(condition),
            body,
            meta,
        }])
    }

    /// The identifier names in an `expression_list` (or a single identifier).
    fn names_in(&self, node: Node) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for c in node.children(&mut node.walk()) {
            match c.kind() {
                "identifier" => out.push(self.base.text(c)?.to_string()),
                "," | "expression_list" => {}
                other => anyhow::bail!("go: unsupported assignment target `{other}`"),
            }
        }
        Ok(out)
    }

    /// The expressions in an `expression_list` (or a single expression).
    fn values_in(&mut self, node: Node) -> Result<Vec<Expression>> {
        let mut out = Vec::new();
        for c in node.children(&mut node.walk()) {
            match c.kind() {
                "," | "expression_list" => {}
                _ => out.push(self.visit_expression(c)?),
            }
        }
        Ok(out)
    }

    fn visit_block(&mut self, node: Node) -> Result<Vec<Statement>> {
        let mut body = Vec::new();
        for child in node.children(&mut node.walk()) {
            body.extend(self.visit_statements(child)?);
        }
        Ok(body)
    }

    fn visit_expression(&mut self, node: Node) -> Result<Expression> {
        let node = self.base.unwrap_parens(node);
        let meta = self.base.create_meta(node, "go", self.file_name);

        match node.kind() {
            "identifier" | "selector_expression" => Ok(Expression::Var {
                name: self.base.text(node)?.to_string(),
                meta,
            }),
            "int_literal" => Ok(Expression::IntLiteral {
                value: self.base.extract_int_literal(node)?,
                meta,
            }),
            "float_literal" => Ok(Expression::FloatLiteral {
                value: self.base.extract_float_literal(node)?,
                meta,
            }),
            "string_literal" | "interpreted_string_literal" | "raw_string_literal" => {
                Ok(Expression::StringLiteral {
                    value: self.base.extract_string_literal(node)?,
                    meta,
                })
            }
            "true" => Ok(Expression::BoolLiteral { value: true, meta }),
            "false" => Ok(Expression::BoolLiteral { value: false, meta }),
            "nil" => Ok(Expression::NullLiteral { meta }),
            "binary_expression" => {
                let left =
                    Box::new(self.visit_expression(node.child_by_field_name("left").unwrap())?);
                let operator = self.base.child_text(node, "operator")?.to_string();
                let right =
                    Box::new(self.visit_expression(node.child_by_field_name("right").unwrap())?);
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
                if let Some(cap_name) = crush_walker_core::map_to_capability("go", func_name) {
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
                    anyhow::bail!(
                        "go: `{}` expressions are not supported by the Crush Go walker",
                        node.kind()
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use crush_walker_core::{frontend_pipeline, TreeSitterFrontend};

    #[test]
    fn test_treesitter_frontend_adapter() {
        let walker = GoWalker {
            file_name: "test.go".to_string(),
        };
        let frontend = TreeSitterFrontend::new(walker, "go", &[".go"]);

        let source = r#"
package main

func main() {
    fmt.Println("hello")
}
"#;

        let (report, program) = frontend_pipeline(&frontend, source).unwrap();
        assert_eq!(report.lang, "go");
        assert_eq!(program.lang.unwrap(), "go");
        assert_eq!(program.entry, "main");
        assert!(program.functions.contains_key("main"));
    }

    #[test]
    fn test_treesitter_frontend_parse_errors() {
        let walker = GoWalker {
            file_name: "bad.go".to_string(),
        };
        let frontend = TreeSitterFrontend::new(walker, "go", &[".go"]);

        // Invalid Go fails loudly now: it used to lower to `null` silently.
        assert!(frontend_pipeline(&frontend, "this is not go").is_err());
    }

    /// Parse `src` as Go and run the walker.
    fn walk_go(src: &str) -> anyhow::Result<crush_cast::Program> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        GoWalker {
            file_name: "t.go".to_string(),
        }
        .walk(&tree, src.as_bytes())
    }

    fn main_body(src: &str) -> Vec<Statement> {
        walk_go(src)
            .unwrap()
            .functions
            .get("main")
            .unwrap()
            .body
            .clone()
    }

    // CRUSH-198: `a := 7` was dropped (the walker matched the wrong node name),
    // so `a` read as uninitialised. It must lower to a `VarDecl` now.
    #[test]
    fn short_var_declaration_is_not_dropped() {
        let body = main_body("package main\nfunc main() {\n a := 7\n fmt.Println(a)\n}\n");
        assert!(
            matches!(body.first(), Some(Statement::VarDecl { name, .. }) if name == "a"),
            "got {body:?}"
        );
    }

    // CRUSH-198: `var`, assignment and C-style `for` must lower, not drop.
    #[test]
    fn var_assignment_and_for_lower() {
        let body = main_body(
            "package main\nfunc main() {\n var b = 3\n b = b + 1\n for i := 0; i < 3; i++ {\n  b = b + i\n }\n}\n",
        );
        assert!(
            matches!(body[0], Statement::VarDecl { .. }),
            "var: {body:?}"
        );
        assert!(
            matches!(body[1], Statement::Assign { .. }),
            "assign: {body:?}"
        );
        // `for i := 0; i < 3; i++` lowers to the initializer then a `while`.
        assert!(
            matches!(body[2], Statement::VarDecl { .. }),
            "for init: {body:?}"
        );
        assert!(
            matches!(body[3], Statement::While { .. }),
            "for body: {body:?}"
        );
    }

    // CRUSH-198: unsupported statements are a loud error, never a silent drop.
    #[test]
    fn unsupported_statements_are_loud() {
        for src in [
            "package main\nfunc main() {\n switch 1 {\n case 1:\n }\n}\n",
            "package main\nfunc main() {\n defer f()\n}\n",
            "package main\nfunc main() {\n for i, v := range xs {\n }\n}\n",
        ] {
            let err = walk_go(src).unwrap_err().to_string();
            assert!(
                err.contains("not supported"),
                "expected a loud error, got: {err}"
            );
        }
    }

    // CRUSH-198: unknown expressions are a loud error, not `NullLiteral`.
    #[test]
    fn unsupported_expressions_are_loud() {
        let err = walk_go("package main\nfunc main() {\n a := []int{1}\n}\n")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not supported"),
            "expected a loud error, got: {err}"
        );
    }

    // CRUSH-198: `return a, b` used to keep only `a` silently.
    #[test]
    fn multiple_return_values_are_loud() {
        let err = walk_go("package main\nfunc f() (int, int) {\n return 1, 2\n}\n")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("not supported"),
            "expected a loud error, got: {err}"
        );
    }
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use crush_walker_core::LanguageAdapter;

pub struct GoAdapter;
impl LanguageAdapter for GoAdapter {
    fn language_name(&self) -> &'static str {
        "go"
    }
    fn file_extensions(&self) -> &[&'static str] {
        &["go"]
    }
    fn walk(
        &self,
        source: &str,
        filename: &str,
    ) -> anyhow::Result<(crush_walker_core::FeatureReport, crush_cast::Program)> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .map_err(|e| anyhow::anyhow!("tree-sitter-go init: {e}"))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Go parse failed"))?;
        let walker = crate::GoWalker {
            file_name: filename.to_string(),
        };
        let program = walker.walk(&tree, source.as_bytes())?;
        Ok((
            crush_walker_core::FeatureReport {
                lang: "go".to_string(),
                ..Default::default()
            },
            program,
        ))
    }
}
pub mod sdk;
