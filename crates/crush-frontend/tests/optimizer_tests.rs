use crush_cast::{Expression, Function, Program, Statement};
use crush_frontend::compile_cast;
use crush_frontend::optimizer::Optimizer;
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

fn optimize_program(program: &mut Program) {
    Optimizer::optimize(program);
}

#[test]
fn test_constant_folding_add() {
    let body = vec![Statement::Return {
        value: Some(Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(Expression::IntLiteral {
                value: 10,
                meta: create_empty_meta(),
            }),
            right: Box::new(Expression::IntLiteral {
                value: 32,
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        }),
        meta: create_empty_meta(),
    }];

    let program = create_program(body);
    let casm = compile_cast(&program).expect("Compilation failed");

    let main_fn = casm.functions.get("main").unwrap();

    // Should be optimized to: push_int 42, ret
    // No 'add' instruction should be present
    assert!(!main_fn.body.iter().any(|i| i.op == "add"));
    assert!(
        main_fn
            .body
            .iter()
            .any(|i| i.op == "push_int" && i.args["value"] == 42)
    );
}

#[test]
fn test_constant_folding_nested() {
    // (1 + 2) * 3 => 9
    let body = vec![Statement::Return {
        value: Some(Expression::BinaryOp {
            operator: "*".to_string(),
            left: Box::new(Expression::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(Expression::IntLiteral {
                    value: 1,
                    meta: create_empty_meta(),
                }),
                right: Box::new(Expression::IntLiteral {
                    value: 2,
                    meta: create_empty_meta(),
                }),
                meta: create_empty_meta(),
            }),
            right: Box::new(Expression::IntLiteral {
                value: 3,
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        }),
        meta: create_empty_meta(),
    }];

    let program = create_program(body);
    let casm = compile_cast(&program).expect("Compilation failed");

    let main_fn = casm.functions.get("main").unwrap();

    assert!(!main_fn.body.iter().any(|i| i.op == "add"));
    assert!(!main_fn.body.iter().any(|i| i.op == "mul"));
    assert!(
        main_fn
            .body
            .iter()
            .any(|i| i.op == "push_int" && i.args["value"] == 9)
    );
}

#[test]
fn test_constant_folding_unary() {
    // -(-42) => 42
    let body = vec![Statement::Return {
        value: Some(Expression::UnaryOp {
            operator: "-".to_string(),
            operand: Box::new(Expression::UnaryOp {
                operator: "-".to_string(),
                operand: Box::new(Expression::IntLiteral {
                    value: 42,
                    meta: create_empty_meta(),
                }),
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        }),
        meta: create_empty_meta(),
    }];

    let program = create_program(body);
    let casm = compile_cast(&program).expect("Compilation failed");

    let main_fn = casm.functions.get("main").unwrap();

    assert!(!main_fn.body.iter().any(|i| i.op == "neg"));
    assert!(
        main_fn
            .body
            .iter()
            .any(|i| i.op == "push_int" && i.args["value"] == 42)
    );
}

#[test]
fn test_constant_propagation_replaces_variable_use() {
    let mut program = create_program(vec![
        Statement::VarDecl {
            name: "x".to_string(),
            value: Expression::IntLiteral {
                value: 5,
                meta: create_empty_meta(),
            },
            type_hint: crush_cast::CastType::Any,
            meta: create_empty_meta(),
        },
        Statement::ExprStmt {
            expr: Expression::BinaryOp {
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
            },
            meta: create_empty_meta(),
        },
    ]);

    optimize_program(&mut program);
    let main = program.functions.get("main").unwrap();
    match &main.body[1] {
        Statement::ExprStmt {
            expr: Expression::IntLiteral { value, .. },
            ..
        } => {
            assert_eq!(*value, 6);
        }
        other => panic!(
            "expected folded int literal after propagation, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_dead_code_elimination_after_return() {
    let mut program = create_program(vec![
        Statement::Return {
            value: Some(Expression::IntLiteral {
                value: 1,
                meta: create_empty_meta(),
            }),
            meta: create_empty_meta(),
        },
        Statement::ExprStmt {
            expr: Expression::IntLiteral {
                value: 99,
                meta: create_empty_meta(),
            },
            meta: create_empty_meta(),
        },
    ]);

    optimize_program(&mut program);
    let main = program.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    assert!(matches!(main.body[0], Statement::Return { .. }));
}

#[test]
fn test_dead_code_elimination_constant_if_branch() {
    let mut program = create_program(vec![Statement::If {
        condition: Expression::BoolLiteral {
            value: false,
            meta: create_empty_meta(),
        },
        then_body: vec![Statement::ExprStmt {
            expr: Expression::IntLiteral {
                value: 1,
                meta: create_empty_meta(),
            },
            meta: create_empty_meta(),
        }],
        else_body: Some(vec![Statement::ExprStmt {
            expr: Expression::IntLiteral {
                value: 2,
                meta: create_empty_meta(),
            },
            meta: create_empty_meta(),
        }]),
        meta: create_empty_meta(),
    }]);

    optimize_program(&mut program);
    let main = program.functions.get("main").unwrap();
    assert_eq!(main.body.len(), 1);
    match &main.body[0] {
        Statement::ExprStmt {
            expr: Expression::IntLiteral { value, .. },
            ..
        } => {
            assert_eq!(*value, 2);
        }
        other => panic!("expected else branch only, got: {:?}", other),
    }
}

#[test]
fn test_strength_reduction_mul_by_two() {
    let mut functions = HashMap::new();
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![("n".to_string(), crush_cast::CastType::Int)],
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    operator: "*".to_string(),
                    left: Box::new(Expression::Var {
                        name: "n".to_string(),
                        meta: create_empty_meta(),
                    }),
                    right: Box::new(Expression::IntLiteral {
                        value: 2,
                        meta: create_empty_meta(),
                    }),
                    meta: create_empty_meta(),
                }),
                meta: create_empty_meta(),
            }],
            meta: create_empty_meta(),
            ..Default::default()
        },
    );
    let mut program = Program {
        cast_version: "1.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    };

    optimize_program(&mut program);
    let main = program.functions.get("main").unwrap();
    match &main.body[0] {
        Statement::Return {
            value:
                Some(Expression::BinaryOp {
                    operator,
                    left,
                    right,
                    ..
                }),
            ..
        } => {
            assert_eq!(operator, "+");
            assert!(matches!(**left, Expression::Var { .. }));
            assert!(matches!(**right, Expression::Var { .. }));
        }
        other => panic!("expected reduced n*2 to n+n, got: {:?}", other),
    }
}

#[test]
fn test_extended_constant_folding_string_and_bool() {
    let mut program = create_program(vec![
        Statement::ExprStmt {
            expr: Expression::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(Expression::StringLiteral {
                    value: "a".to_string(),
                    meta: create_empty_meta(),
                }),
                right: Box::new(Expression::StringLiteral {
                    value: "b".to_string(),
                    meta: create_empty_meta(),
                }),
                meta: create_empty_meta(),
            },
            meta: create_empty_meta(),
        },
        Statement::ExprStmt {
            expr: Expression::UnaryOp {
                operator: "!".to_string(),
                operand: Box::new(Expression::BoolLiteral {
                    value: true,
                    meta: create_empty_meta(),
                }),
                meta: create_empty_meta(),
            },
            meta: create_empty_meta(),
        },
    ]);

    optimize_program(&mut program);
    let main = program.functions.get("main").unwrap();
    match &main.body[0] {
        Statement::ExprStmt {
            expr: Expression::StringLiteral { value, .. },
            ..
        } => {
            assert_eq!(value, "ab");
        }
        other => panic!("expected string concat folding, got: {:?}", other),
    }
    match &main.body[1] {
        Statement::ExprStmt {
            expr: Expression::BoolLiteral { value, .. },
            ..
        } => {
            assert!(!value);
        }
        other => panic!("expected bool unary folding, got: {:?}", other),
    }
}
