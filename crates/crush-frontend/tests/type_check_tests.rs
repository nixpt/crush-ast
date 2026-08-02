use crush_cast::CastType;
use crush_cast::{Expression, Function, Program, Statement};
use crush_frontend::compile_cast;
use crush_frontend::semantics::SemanticAnalyzer;
use std::collections::HashMap;

fn create_empty_meta() -> HashMap<String, serde_json::Value> {
    HashMap::new()
}

fn create_program(body: Vec<Statement>) -> Program {
    let mut functions = HashMap::new();
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            body,
            meta: create_empty_meta(),
            ..Default::default()
        },
    );

    Program {
        cast_version: "1.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    }
}

fn create_program_with_functions(functions: HashMap<String, Function>) -> Program {
    Program {
        cast_version: "1.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    }
}

#[test]
fn test_type_mismatch_var_decl() {
    let body = vec![Statement::VarDecl {
        name: "x".to_string(),
        value: Expression::StringLiteral {
            value: "hello".to_string(),
            meta: create_empty_meta(),
        },
        type_hint: CastType::Int,
        meta: create_empty_meta(),
    }];

    let program = create_program(body);
    let result = compile_cast(&program);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Type mismatch"));
}

#[test]
fn test_valid_type_var_decl() {
    let body = vec![Statement::VarDecl {
        name: "x".to_string(),
        value: Expression::IntLiteral {
            value: 42,
            meta: create_empty_meta(),
        },
        type_hint: CastType::Int,
        meta: create_empty_meta(),
    }];

    let program = create_program(body);
    let result = compile_cast(&program);

    assert!(result.is_ok());
}

#[test]
fn test_if_condition_type_check() {
    let body = vec![Statement::If {
        condition: Expression::IntLiteral {
            value: 42,
            meta: create_empty_meta(),
        },
        then_body: vec![],
        else_body: None,
        meta: create_empty_meta(),
    }];

    let program = create_program(body);
    let result = compile_cast(&program);

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("If condition must be bool")
    );
}

#[test]
fn test_struct_field_access() {
    let body = vec![
        Statement::StructDef {
            name: "User".to_string(),
            fields: vec![("name".to_string(), CastType::String)],
            meta: create_empty_meta(),
        },
        Statement::VarDecl {
            name: "u".to_string(),
            value: Expression::NewStruct {
                name: "User".to_string(),
                meta: create_empty_meta(),
            },
            type_hint: CastType::TypeRef("User".to_string()),
            meta: create_empty_meta(),
        },
        Statement::VarDecl {
            name: "n".to_string(),
            value: Expression::GetField {
                target: Box::new(Expression::Var {
                    name: "u".to_string(),
                    meta: create_empty_meta(),
                }),
                field: "name".to_string(),
                meta: create_empty_meta(),
            },
            type_hint: CastType::String,
            meta: create_empty_meta(),
        },
    ];

    let program = create_program(body);
    let result = compile_cast(&program);

    assert!(result.is_ok());
}

#[test]
fn test_invalid_struct_field_access() {
    let body = vec![
        Statement::StructDef {
            name: "User".to_string(),
            fields: vec![("name".to_string(), CastType::String)],
            meta: create_empty_meta(),
        },
        Statement::VarDecl {
            name: "u".to_string(),
            value: Expression::NewStruct {
                name: "User".to_string(),
                meta: create_empty_meta(),
            },
            type_hint: CastType::TypeRef("User".to_string()),
            meta: create_empty_meta(),
        },
        Statement::ExprStmt {
            expr: Expression::GetField {
                target: Box::new(Expression::Var {
                    name: "u".to_string(),
                    meta: create_empty_meta(),
                }),
                field: "age".to_string(),
                meta: create_empty_meta(),
            },
            meta: create_empty_meta(),
        },
    ];

    let program = create_program(body);
    let result = compile_cast(&program);

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("has no field 'age'")
    );
}

#[test]
fn test_function_return_type_inference_allows_numeric_use() {
    let mut functions = HashMap::new();
    functions.insert(
        "helper".to_string(),
        Function {
            params: vec![],
            body: vec![Statement::Return {
                value: Some(Expression::IntLiteral {
                    value: 5,
                    meta: create_empty_meta(),
                }),
                meta: create_empty_meta(),
            }],
            meta: create_empty_meta(),
            ..Default::default()
        },
    );
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            body: vec![Statement::ExprStmt {
                expr: Expression::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(Expression::Call {
                        function: "helper".to_string(),
                        args: vec![],
                        meta: create_empty_meta(),
                    }),
                    right: Box::new(Expression::IntLiteral {
                        value: 2,
                        meta: create_empty_meta(),
                    }),
                    meta: create_empty_meta(),
                },
                meta: create_empty_meta(),
            }],
            meta: create_empty_meta(),
            ..Default::default()
        },
    );

    let program = create_program_with_functions(functions);
    let result = compile_cast(&program);
    assert!(
        result.is_ok(),
        "inferred int return should support int arithmetic"
    );
}

#[test]
fn test_conflicting_function_return_types_fail() {
    let mut functions = HashMap::new();
    functions.insert(
        "helper".to_string(),
        Function {
            params: vec![],
            body: vec![
                Statement::Return {
                    value: Some(Expression::IntLiteral {
                        value: 1,
                        meta: create_empty_meta(),
                    }),
                    meta: create_empty_meta(),
                },
                Statement::Return {
                    value: Some(Expression::StringLiteral {
                        value: "oops".to_string(),
                        meta: create_empty_meta(),
                    }),
                    meta: create_empty_meta(),
                },
            ],
            meta: create_empty_meta(),
            ..Default::default()
        },
    );
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            body: vec![Statement::ExprStmt {
                expr: Expression::Call {
                    function: "helper".to_string(),
                    args: vec![],
                    meta: create_empty_meta(),
                },
                meta: create_empty_meta(),
            }],
            meta: create_empty_meta(),
            ..Default::default()
        },
    );

    let program = create_program_with_functions(functions);
    let result = compile_cast(&program);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Conflicting return types")
    );
}

#[test]
fn test_infer_expression_type_uses_main_scope() {
    let body = vec![Statement::VarDecl {
        name: "x".to_string(),
        value: Expression::IntLiteral {
            value: 7,
            meta: create_empty_meta(),
        },
        type_hint: CastType::Any,
        meta: create_empty_meta(),
    }];
    let program = create_program(body);
    let expr = Expression::BinaryOp {
        operator: "+".to_string(),
        left: Box::new(Expression::Var {
            name: "x".to_string(),
            meta: create_empty_meta(),
        }),
        right: Box::new(Expression::IntLiteral {
            value: 1,
            meta: create_empty_meta(),
        }),
        meta: create_empty_meta(),
    };

    let mut analyzer = SemanticAnalyzer::new();
    let ty = analyzer
        .infer_expression_type(&program, &expr)
        .expect("inference should succeed");
    assert_eq!(ty, crush_frontend::types::Type::Int);
}

#[test]
fn test_binary_type_check_int_plus_float_allowed() {
    let body = vec![Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(Expression::IntLiteral {
                value: 1,
                meta: create_empty_meta(),
            }),
            right: Box::new(Expression::FloatLiteral {
                value: 2.5,
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        },
        meta: create_empty_meta(),
    }];
    let result = compile_cast(&create_program(body));
    assert!(
        result.is_ok(),
        "int + float should be valid and infer float"
    );
}

#[test]
fn test_binary_type_check_string_concat_allowed() {
    let body = vec![Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(Expression::StringLiteral {
                value: "hello".to_string(),
                meta: create_empty_meta(),
            }),
            right: Box::new(Expression::StringLiteral {
                value: " world".to_string(),
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        },
        meta: create_empty_meta(),
    }];
    let result = compile_cast(&create_program(body));
    assert!(result.is_ok(), "string + string should be valid");
}

#[test]
fn test_binary_type_check_logical_ops_require_bool() {
    let good_body = vec![Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "&&".to_string(),
            left: Box::new(Expression::BoolLiteral {
                value: true,
                meta: create_empty_meta(),
            }),
            right: Box::new(Expression::BoolLiteral {
                value: false,
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        },
        meta: create_empty_meta(),
    }];
    assert!(compile_cast(&create_program(good_body)).is_ok());

    let bad_body = vec![Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "||".to_string(),
            left: Box::new(Expression::IntLiteral {
                value: 1,
                meta: create_empty_meta(),
            }),
            right: Box::new(Expression::BoolLiteral {
                value: true,
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        },
        meta: create_empty_meta(),
    }];
    let result = compile_cast(&create_program(bad_body));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("requires bool operands")
    );
}

// ── Return-type inference: call-graph SCC ordering ──────────────────────────

/// A call chain deeper than the old 10-iteration global fixed point could
/// resolve. Before SCC-ordered inference, chains deeper than ~12 functions
/// nondeterministically (HashMap iteration order) kept placeholder Null
/// return types, making `chain39(1) > 0` fail with "Cannot compare types".
#[test]
fn deep_call_chain_return_types_resolve() {
    let mut source = String::from("fn chain00(x: Int) { return x + 42 }\n");
    for i in 1..40 {
        source.push_str(&format!(
            "fn chain{:02}(x: Int) {{ return chain{:02}(x) + 1 }}\n",
            i,
            i - 1
        ));
    }
    source.push_str("fn main() {\n    if chain39(1) > 0 {\n        return 1\n    }\n    return 0\n}\n");
    crush_frontend::compile_crush_source(&source)
        .expect("deep call chain must type-check deterministically");
}

/// Mutual recursion forms a genuine SCC; its members must still converge via
/// the scoped fixed point.
#[test]
fn mutual_recursion_return_types_resolve() {
    let source = r#"
fn is_even(n: Int) {
    if n == 0 {
        return true
    }
    return is_odd(n - 1)
}

fn is_odd(n: Int) {
    if n == 0 {
        return false
    }
    return is_even(n - 1)
}

fn main() {
    if is_even(10) {
        return 1
    }
    return 0
}
"#;
    crush_frontend::compile_crush_source(source)
        .expect("mutual recursion must type-check");
}
