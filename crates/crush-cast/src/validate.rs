use std::fmt;

/// A validation error with JSON path information.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// JSON path to the error location (e.g., "functions.main.body[3].AIExpression.Query.query")
    pub path: String,
    /// Human-readable error description
    pub message: String,
    /// Optional hint for common mistakes
    pub hint: Option<String>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {}: {}", self.path, self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  hint: {}", hint)?;
        }
        Ok(())
    }
}

/// Common author mistakes and their fixes
///
/// Conditions match on the serde error message, not the JSON path:
/// serde_path_to_error buffers internally-tagged enum content, so errors in
/// nested positions surface with a truncated path like "functions.main.body[0]".
fn suggest_fix(error_msg: &str, path: &str) -> Option<String> {
    // Check for Print statement (common mistake: doesn't exist)
    if error_msg.contains("unknown variant `Print`") {
        return Some("Statement type 'Print' doesn't exist. Use ExprStmt with CapabilityCall(\"print\") instead (a Call to \"print\" validates but does not compile — print is a capability, not a function).\nExample: {\"type\": \"ExprStmt\", \"expr\": {\"type\": \"CapabilityCall\", \"name\": \"print\", \"args\": [...], \"meta\": {}}}".to_string());
    }

    // Check for Identifier expression (common mistake: doesn't exist)
    if error_msg.contains("unknown variant `Identifier`") {
        return Some("Expression type 'Identifier' doesn't exist. Use 'Var' instead.\nExample: {\"type\": \"Var\", \"name\": \"variable_name\"}".to_string());
    }

    // Check for Literal expression (common mistake: doesn't exist)
    if error_msg.contains("unknown variant `Literal`") {
        return Some("Expression type 'Literal' doesn't exist. Use 'StringLiteral', 'IntLiteral', 'FloatLiteral', or 'BoolLiteral' instead.\nExamples:\n  String: {\"type\": \"StringLiteral\", \"value\": \"hello\"}\n  Int: {\"type\": \"IntLiteral\", \"value\": 42}".to_string());
    }

    // Check for missing Function.meta
    if error_msg.contains("missing field") && error_msg.contains("meta") && path.contains("functions") {
        return Some("Field 'meta' is required in Function definitions. Add: \"meta\": {}".to_string());
    }

    // Check for prompt vs query in AI Query. serde ignores the unknown
    // "prompt" field, so the error that actually surfaces is the missing
    // required "query" field.
    if error_msg.contains("missing field `query`") {
        return Some("AI Query requires the field 'query' (a 'prompt' field is ignored — it doesn't exist in the schema).\nCorrect form: {\"ai_type\": \"Query\", \"query\": \"your question here\", ...}".to_string());
    }

    // Check for DelegationStrategy (common mistake: wrong shape).
    // {"type": "Best"} deserializes as unknown variant `type` against the
    // DelegationStrategy variant list — match on that list's first entry.
    if error_msg.contains("unknown variant") && error_msg.contains("`FirstAvailable`") {
        return Some("DelegationStrategy should be a plain string, not an object.\nIncorrect: {\"type\": \"Best\"}\nCorrect: \"Best\"\nValid values: FirstAvailable, CapabilityMatch, ParallelSplit, Hierarchical, Consensus, Broadcast, Best, RoundRobin".to_string());
    }

    None
}


/// Validate a CAST JSON string against the schema.
///
/// Returns `Ok(())` if the JSON deserializes into a valid `Program`.
/// Returns `Err(Vec<ValidationError>)` with user-friendly errors including JSON paths.
///
/// Note: serde stops at the first deserialization error, so this reports the first
/// error encountered. For multi-error validation, a custom two-pass validator would
/// be needed (not implemented here).
pub fn validate_json(s: &str) -> Result<(), Vec<ValidationError>> {
    // Pass 1: Basic JSON syntax check
    if let Err(e) = serde_json::from_str::<serde_json::Value>(s) {
        return Err(vec![ValidationError {
            path: format!("line {}", e.line()),
            message: format!("invalid JSON: {}", e),
            hint: None,
        }]);
    }

    // Pass 2: Deserialize into Program with path tracking
    let deserializer = &mut serde_json::Deserializer::from_str(s);
    match serde_path_to_error::deserialize::<_, crate::Program>(deserializer) {
        Ok(_) => Ok(()),
        Err(e) => {
            let path = e.path().to_string();
            let inner = e.inner().to_string();
            let path_display = if path.is_empty() {
                "(root)".to_string()
            } else {
                path.clone()
            };
            let hint = suggest_fix(&inner, &path);
            Err(vec![ValidationError {
                path: path_display,
                message: inner,
                hint,
            }])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_program() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "lang": null,
            "functions": {
                "main": {
                    "params": [],
                    "body": [],
                    "meta": {}
                }
            },
            "ai_meta": null
        }"#;
        assert!(validate_json(json).is_ok());
    }

    #[test]
    fn test_missing_required_field() {
        let json = r#"{
            "cast_version": "0.1.0",
            "functions": {}
        }"#;
        let err = validate_json(json).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].message.contains("entry") || err[0].message.contains("missing"));
    }

    #[test]
    fn test_wrong_type() {
        let json = r#"{
            "cast_version": 42,
            "entry": "main",
            "functions": {}
        }"#;
        let err = validate_json(json).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].path.contains("cast_version") || err[0].message.contains("string"));
    }

    #[test]
    fn test_wrong_statement_tag() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{"type": "NonExistent"}],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].message.contains("NonExistent") || err[0].message.contains("variant"));
    }

    #[test]
    fn test_extra_unknown_field() {
        // serde by default ignores unknown fields — should still be valid
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {},
            "unknown_field": "should be ignored"
        }"#;
        assert!(validate_json(json).is_ok());
    }

    #[test]
    fn test_nested_error_path() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {"type": "NotARealExpr"},
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].path.contains("body") || err[0].path.contains("value"));
        assert!(err[0].message.contains("NotARealExpr") || err[0].message.contains("variant"));
    }

    #[test]
    fn test_print_statement_hint() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{"type": "Print", "args": []}],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].hint.is_some());
        assert!(err[0].hint.as_ref().unwrap().contains("ExprStmt"));
        assert!(err[0].hint.as_ref().unwrap().contains("CapabilityCall"));
    }

    #[test]
    fn test_identifier_expression_hint() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {"type": "Identifier", "name": "y"},
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].hint.is_some());
        assert!(err[0].hint.as_ref().unwrap().contains("Var"));
    }

    #[test]
    fn test_literal_expression_hint() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {"type": "Literal", "value": "hello"},
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].hint.is_some());
        assert!(err[0].hint.as_ref().unwrap().contains("StringLiteral"));
    }

    #[test]
    fn test_missing_function_meta() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": []
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].hint.is_some());
        assert!(err[0].hint.as_ref().unwrap().contains("meta"));
    }

    #[test]
    fn test_ai_query_prompt_field_hint() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {
                            "type": "AI",
                            "ai_type": "Query",
                            "prompt": "What is this?"
                        },
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].hint.is_some());
        assert!(err[0].hint.as_ref().unwrap().contains("query"));
    }

    #[test]
    fn test_delegation_strategy_shape_hint() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {
                            "type": "AI",
                            "ai_type": "AgentDelegation",
                            "task": "do something",
                            "agents": ["a"],
                            "delegation_strategy": {"type": "Best"}
                        },
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        let err = validate_json(json).unwrap_err();
        assert!(err[0].hint.is_some());
        assert!(err[0].hint.as_ref().unwrap().contains("plain string"));
    }

    #[test]
    fn test_valid_print_pattern() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "ExprStmt",
                        "expr": {
                            "type": "Call",
                            "function": "print",
                            "args": [{"type": "StringLiteral", "value": "hello"}]
                        },
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        assert!(validate_json(json).is_ok());
    }

    #[test]
    fn test_valid_var_expression() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {"type": "Var", "name": "y"},
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        assert!(validate_json(json).is_ok());
    }

    #[test]
    fn test_valid_delegation_strategy() {
        let json = r#"{
            "cast_version": "0.1.0",
            "entry": "main",
            "functions": {
                "main": {
                    "params": [],
                    "body": [{
                        "type": "VarDecl",
                        "name": "x",
                        "value": {
                            "type": "AI",
                            "ai_type": "AgentDelegation",
                            "task": "do something",
                            "agents": ["a"],
                            "delegation_strategy": "Best"
                        },
                        "meta": {}
                    }],
                    "meta": {}
                }
            }
        }"#;
        assert!(validate_json(json).is_ok());
    }
}
