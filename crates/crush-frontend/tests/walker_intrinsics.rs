use crush_cast::{CastType, Expression, Function, Program, Statement};
use crush_frontend::{compile_cast_owned, compiler::Compiler};

fn var(name: &str) -> Expression {
    Expression::Var {
        name: name.into(),
        meta: Default::default(),
    }
}
fn int(value: i64) -> Expression {
    Expression::IntLiteral {
        value,
        meta: Default::default(),
    }
}
fn call(function: &str, args: Vec<Expression>) -> Expression {
    Expression::Call {
        function: function.into(),
        args,
        meta: Default::default(),
    }
}
fn stmt(expr: Expression) -> Statement {
    Statement::ExprStmt {
        expr,
        meta: Default::default(),
    }
}
fn decl(name: &str, value: Expression) -> Statement {
    Statement::VarDecl {
        name: name.into(),
        value,
        type_hint: CastType::Any,
        meta: Default::default(),
    }
}
fn program(body: Vec<Statement>) -> Program {
    Program {
        entry: "main".into(),
        functions: [(
            "main".into(),
            Function {
                body,
                ..Default::default()
            },
        )]
        .into(),
        ..Default::default()
    }
}

#[test]
fn assignment_survives_semantics_and_constant_propagation() {
    let casm = compile_cast_owned(program(vec![
        decl("x", int(1)),
        stmt(call("__crush_assign__", vec![var("x"), int(2)])),
        stmt(call("print", vec![var("x")])),
    ]))
    .unwrap();
    let ops = &casm.functions["main"].body;
    assert!(
        ops.iter()
            .any(|op| op.op == "store" && op.args["name"] == "x")
    );
    assert!(!ops.iter().any(|op| op.args["name"] == "__crush_assign__"));
    assert!(
        ops.iter()
            .any(|op| op.op == "load" && op.args["name"] == "x")
    );
}

#[test]
fn assignment_target_and_arity_are_checked_by_both_entry_points() {
    for (args, message) in [
        (vec![var("x")], "expects exactly 2 arguments"),
        (vec![int(1), int(2)], "target must be a variable"),
    ] {
        let p = program(vec![stmt(call("__crush_assign__", args))]);
        assert!(
            compile_cast_owned(p.clone())
                .unwrap_err()
                .to_string()
                .contains(message)
        );
        assert!(
            Compiler::new()
                .compile(p)
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
}

#[test]
fn unknown_helpers_remain_errors() {
    let error =
        compile_cast_owned(program(vec![stmt(call("__crush_unknown__", vec![]))])).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Undefined function: __crush_unknown__")
    );
}

fn for_loop(iterable: Expression, body: Vec<Statement>) -> Statement {
    Statement::For {
        variable: "item".into(),
        iterable: Box::new(iterable),
        body,
        meta: Default::default(),
    }
}

#[test]
fn for_binding_is_visible_in_body_and_does_not_leak() {
    let iterable = Expression::ArrayLiteral {
        elements: vec![int(1)],
        meta: Default::default(),
    };
    let body = vec![for_loop(
        iterable,
        vec![stmt(call("print", vec![var("item")]))],
    )];
    compile_cast_owned(program(body.clone())).unwrap();
    let mut leaking = body;
    leaking.push(stmt(call("print", vec![var("item")])));
    assert!(
        compile_cast_owned(program(leaking))
            .unwrap_err()
            .to_string()
            .contains("Undefined variable: item")
    );
}

#[test]
fn for_body_and_range_bounds_are_checked() {
    let bad_body = program(vec![for_loop(int(1), vec![])]);
    assert!(
        compile_cast_owned(bad_body)
            .unwrap_err()
            .to_string()
            .contains("For requires an iterable")
    );
    let range = Expression::Range {
        start: Box::new(int(0)),
        end: Box::new(var("missing")),
        meta: Default::default(),
    };
    assert!(
        compile_cast_owned(program(vec![for_loop(range, vec![])]))
            .unwrap_err()
            .to_string()
            .contains("Undefined variable: missing")
    );
    let array = Expression::ArrayLiteral {
        elements: vec![int(1)],
        meta: Default::default(),
    };
    let error = compile_cast_owned(program(vec![for_loop(
        array,
        vec![stmt(call("missing", vec![]))],
    )]))
    .unwrap_err();
    assert!(error.to_string().contains("Undefined function: missing"));
}

#[test]
fn struct_field_iteration_keeps_the_existing_map_contract() {
    crush_frontend::compile_crush_source(
        "struct P { x } fn main() { let p = new P(); for name in p { print(name); } }",
    )
    .unwrap();
}
