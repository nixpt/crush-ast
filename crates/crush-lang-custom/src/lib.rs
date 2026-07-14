//! `crush-lang-custom` — A meta-frontend that allows anyone to define a custom language
//! using CSON grammar rules and parse/lower it dynamically to CAST.

use std::any::Any;
use std::collections::HashMap;
use regex::Regex;
use anyhow::{Result, anyhow};
use crush_cast::{Program, Statement, Expression, Function, CastType};
use crush_cson::CsonValue;
use crush_cson::parser::CsonParser;
use walker_core::{Frontend, FeatureReport};

/// Rule matching structure mapping a regex to a CAST node type.
#[derive(Debug, Clone)]
pub struct CustomRule {
    pub name: String,
    pub pattern: Regex,
    pub node_type: String,
    pub capability: Option<String>,
    pub mappings: HashMap<String, String>,
}

/// Dynamic customizable frontend defined by a CSON file.
pub struct CustomFrontend {
    pub name: String,
    pub extensions: Vec<String>,
    pub rules: Vec<CustomRule>,
}

impl CustomFrontend {
    /// Load a CustomFrontend from a CSON definition string.
    pub fn from_cson(cson_str: &str) -> Result<Self> {
        let mut parser = CsonParser::new(cson_str);
        let doc = parser.parse().map_err(|e| anyhow!("CSON parse error: {}", e))?;
        
        let CsonValue::Object(root_map) = doc.root.value else {
            return Err(anyhow!("Root of custom grammar must be an object"));
        };

        // Parse grammar config
        let mut lang_name = "custom".to_string();
        let mut extensions = vec![".custom".to_string()];

        if let Some(grammar_node) = root_map.get("grammar") {
            if let CsonValue::Object(grammar_map) = &grammar_node.value {
                if let Some(node) = grammar_map.get("language") {
                    if let CsonValue::String(s) = &node.value {
                        lang_name = s.clone();
                    }
                }
                if let Some(node) = grammar_map.get("extensions") {
                    if let CsonValue::Array(arr) = &node.value {
                        extensions = arr.iter().filter_map(|v| {
                            if let CsonValue::String(s) = &v.value { Some(s.clone()) } else { None }
                        }).collect();
                    }
                }
            }
        }

        // Parse rules
        let mut rules = Vec::new();
        if let Some(rules_node) = root_map.get("rules") {
            if let CsonValue::Object(rules_map) = &rules_node.value {
                for (key, val_node) in rules_map {
                    let rule_name = key.clone();
                    if let CsonValue::Object(rule_obj) = &val_node.value {
                        let pattern_str = match rule_obj.get("pattern") {
                            Some(n) => match &n.value {
                                CsonValue::String(s) => s.clone(),
                                _ => continue,
                            },
                            None => continue,
                        };
                        let node_type = match rule_obj.get("node") {
                            Some(n) => match &n.value {
                                CsonValue::String(s) => s.clone(),
                                _ => "ExprStmt".to_string(),
                            },
                            None => "ExprStmt".to_string(),
                        };
                        let capability = match rule_obj.get("capability") {
                            Some(n) => match &n.value {
                                CsonValue::String(s) => Some(s.clone()),
                                _ => None,
                            },
                            None => None,
                        };

                        let mut mappings = HashMap::new();
                        if let Some(m_node) = rule_obj.get("mappings") {
                            if let CsonValue::Object(m_map) = &m_node.value {
                                for (k, v) in m_map {
                                    if let CsonValue::String(val_str) = &v.value {
                                        mappings.insert(k.clone(), val_str.clone());
                                    }
                                }
                            }
                        }

                        let pattern = Regex::new(&pattern_str)
                            .map_err(|e| anyhow!("Invalid regex in rule '{}': {}", rule_name, e))?;

                        rules.push(CustomRule {
                            name: rule_name,
                            pattern,
                            node_type,
                            capability,
                            mappings,
                        });
                    }
                }
            }
        }

        Ok(CustomFrontend {
            name: lang_name,
            extensions,
            rules,
        })
    }

    /// Parse source code using the rules and build a CAST program.
    pub fn parse_to_program(&self, source: &str) -> Result<Program> {
        let mut main_body = Vec::new();

        for (_line_idx, line) in source.lines().enumerate() {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() || line_trimmed.starts_with('#') || line_trimmed.starts_with("//") {
                continue;
            }

            let mut matched = false;
            let meta = HashMap::new(); // In a real parser we'd construct position metadata

            for rule in &self.rules {
                if let Some(caps) = rule.pattern.captures(line_trimmed) {
                    matched = true;

                    match rule.node_type.as_str() {
                        "VarDecl" => {
                            let name_cap = rule.mappings.get("name")
                                .and_then(|key| caps.name(key))
                                .map(|m| m.as_str().to_string())
                                .unwrap_or_else(|| "x".to_string());
                            let val_cap = rule.mappings.get("value")
                                .and_then(|key| caps.name(key))
                                .map(|m| m.as_str())
                                .unwrap_or("null");

                            let value = if let Ok(i) = val_cap.parse::<i64>() {
                                Expression::IntLiteral { value: i, meta: meta.clone() }
                            } else if let Ok(f) = val_cap.parse::<f64>() {
                                Expression::FloatLiteral { value: f, meta: meta.clone() }
                            } else {
                                Expression::StringLiteral { value: val_cap.trim_matches('"').to_string(), meta: meta.clone() }
                            };

                            main_body.push(Statement::VarDecl {
                                name: name_cap,
                                value,
                                type_hint: CastType::Any,
                                meta: meta.clone(),
                            });
                        }
                        "CapabilityCall" => {
                            let cap_name = rule.capability.clone().unwrap_or_else(|| "io.print".to_string());
                            let mut args = Vec::new();
                            if let Some(arg_key) = rule.mappings.get("args") {
                                if let Some(m) = caps.name(arg_key) {
                                    let arg_val = m.as_str().trim_matches('"');
                                    if let Ok(i) = arg_val.parse::<i64>() {
                                        args.push(Expression::IntLiteral { value: i, meta: meta.clone() });
                                    } else {
                                        args.push(Expression::StringLiteral { value: arg_val.to_string(), meta: meta.clone() });
                                    }
                                }
                            }

                            main_body.push(Statement::ExprStmt {
                                expr: Expression::CapabilityCall {
                                    name: cap_name,
                                    args,
                                    meta: meta.clone(),
                                },
                                meta: meta.clone(),
                            });
                        }
                        _ => {}
                    }
                    break;
                }
            }

            if !matched {
                // If line doesn't match any rule, we can optionally warn or emit a dummy statement
            }
        }

        let mut functions = HashMap::new();
        functions.insert("main".to_string(), Function {
            params: vec![],
            body: main_body,
            meta: HashMap::new(),
            is_async: false,
            annotations: None,
        });

        Ok(Program {
            cast_version: "0.2".to_string(),
            entry: "main".to_string(),
            lang: Some(self.name.clone()),
            functions,
            ai_meta: None,
            manifest: None,
            exhaustive_sites: vec![],
            wip: None,
            temporaries: vec![],
            decisions: vec![],
        })
    }
}

impl Frontend for CustomFrontend {
    fn language_name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn file_extensions(&self) -> &[&'static str] {
        let exts: Vec<&'static str> = self.extensions.iter()
            .map(|s| Box::leak(s.clone().into_boxed_str()) as &'static str)
            .collect();
        Box::leak(exts.into_boxed_slice())
    }

    fn parse(&self, source: &str) -> Result<Box<dyn Any>> {
        let prog = self.parse_to_program(source)?;
        Ok(Box::new(prog))
    }

    fn analyze(&self, _parsed: &Box<dyn Any>) -> Result<FeatureReport> {
        let mut report = FeatureReport::default();
        report.lang = self.name.clone();
        Ok(report)
    }

    fn lower(&self, parsed: Box<dyn Any>) -> Result<Program> {
        if let Ok(prog) = parsed.downcast::<Program>() {
            Ok(*prog)
        } else {
            Err(anyhow!("Lowering failed: invalid AST type"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_dsl_frontend() {
        let grammar_cson = r#"
        @cson { version: "1.0" }
        [grammar]
        language: "mini"
        extensions: [".mini"]

        [rules]
        var_decl: {
            pattern: "^let (?P<name>\\w+) = (?P<value>\\d+)$"
            node: "VarDecl"
            mappings: {
                name: "name"
                value: "value"
            }
        }
        print: {
            pattern: "^say (?P<msg>.*)$"
            node: "CapabilityCall"
            capability: "io.print"
            mappings: {
                args: "msg"
            }
        }
        "#;

        let frontend = CustomFrontend::from_cson(grammar_cson).unwrap();
        println!("Grammar name: {}", frontend.name);
        println!("Grammar exts: {:?}", frontend.extensions);
        for rule in &frontend.rules {
            println!("Rule: {}, pattern: {}, node: {}", rule.name, rule.pattern, rule.node_type);
        }

        let source = "let x = 42\nsay \"hello\"";
        let program = frontend.parse_to_program(source).unwrap();
        println!("Program functions main body: {:?}", program.functions.get("main").unwrap().body);
        
        assert_eq!(program.entry, "main");
        let main = program.functions.get("main").unwrap();
        assert_eq!(main.body.len(), 2);
        
        // Check first statement: VarDecl (x = 42)
        if let Statement::VarDecl { name, value, .. } = &main.body[0] {
            assert_eq!(name, "x");
            if let Expression::IntLiteral { value: val, .. } = value {
                assert_eq!(*val, 42);
            } else { panic!(); }
        } else { panic!(); }

        // Check second statement: CapabilityCall (io.print "hello")
        if let Statement::ExprStmt { expr: Expression::CapabilityCall { name, args, .. }, .. } = &main.body[1] {
            assert_eq!(name, "io.print");
            assert_eq!(args.len(), 1);
            if let Expression::StringLiteral { value: val, .. } = &args[0] {
                assert_eq!(val, "hello");
            } else { panic!(); }
        } else { panic!(); }
    }
}
