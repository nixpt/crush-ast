use crush_frontend::{parse_source, render::render_program};

/// Helper: parse → render → parse, then compare ASTs via canonical JSON.
fn assert_roundtrip(source: &str) {
    let original = parse_source(source).expect("initial parse should succeed");
    let rendered = render_program(&original);
    let reparsed = parse_source(&rendered).expect("re-parse of rendered text should succeed");

    let orig_json = serde_json::to_string_pretty(&original).expect("serialize original");
    let rep_json = serde_json::to_string_pretty(&reparsed).expect("serialize reparsed");

    let orig_val: serde_json::Value = serde_json::from_str(&orig_json).expect("deserialize original");
    let rep_val: serde_json::Value = serde_json::from_str(&rep_json).expect("deserialize reparsed");

    assert_eq!(
        orig_val, rep_val,
        "AST mismatch after round-trip.\nRendered:\n{}",
        rendered
    );
}

#[test]
fn roundtrip_arithmetic_and_literals() {
    assert_roundtrip(r#"
let a = 1 + 2 * 3
let b = 10 - 4 / 2
let c = a > b
let d = c == false
"#);
}

#[test]
fn roundtrip_variables_and_exports() {
    assert_roundtrip(r#"
let x = 42
let y = "hello"
export x
export z = 100
"#);
}

#[test]
fn roundtrip_functions_and_calls() {
    assert_roundtrip(r#"
fn add(a: Int, b: Int) {
    return a + b
}

fn main() {
    let result = add(1, 2)
    return result
}
"#);
}

#[test]
fn roundtrip_if_else() {
    assert_roundtrip(r#"
if x > 0 {
    let a = 1
} else if x < 0 {
    let b = -1
} else {
    let c = 0
}
"#);
}

#[test]
fn roundtrip_while_and_for() {
    assert_roundtrip(r#"
let i = 0
while i < 10 {
    let i = i + 1
}

for item in items {
    print(item)
}
"#);
}

#[test]
fn roundtrip_try_catch_throw() {
    assert_roundtrip(r#"
try {
    risky()
} catch err {
    print(err)
    throw err
}
"#);
}

#[test]
fn roundtrip_arrays_objects_indexing() {
    assert_roundtrip(r#"
let arr = [1, 2, 3]
let obj = {name: "test", value: 42}
let first = arr[0]
let n = obj.name
"#);
}

#[test]
fn roundtrip_match_expression() {
    assert_roundtrip(r#"
let result = match x { 1 -> "one", 2 -> "two", _ -> "other" }
"#);
}

#[test]
fn roundtrip_spawn_await() {
    assert_roundtrip(r#"
fn double(n: Int) {
    return n * 2
}

let task = spawn double(5)
let val = await task
"#);
}

#[test]
fn roundtrip_imports_and_structs() {
    assert_roundtrip(r#"
import std.io
import std.math { sqrt }
import helpers as h

struct Point { x: Int, y: Int }

let px = 1
"#);
}

#[test]
fn roundtrip_break_continue() {
    assert_roundtrip(r#"
while true {
    if done {
        break
    }
    if skip {
        continue
    }
    work()
}
"#);
}

/// AI-native nodes should render but not be parseable.
#[test]
fn render_ai_native_expression() {
    let program = crush_cast::Program {
        cast_version: "1.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        functions: {
            let mut m = std::collections::HashMap::new();
            m.insert(
                "main".to_string(),
                crush_cast::Function {
                    params: vec![],
                    body: vec![crush_cast::Statement::ExprStmt {
                        expr: crush_cast::Expression::AI(
                            crush_cast::ai::AIExpression::Query {
                                query: "find users".to_string(),
                                result_type: Some("List<User>".to_string()),
                                context: std::collections::HashMap::new(),
                            },
                        ),
                        meta: std::collections::HashMap::new(),
                    }],
                    meta: std::collections::HashMap::new(),
                },
            );
            m
        },
        ai_meta: None,
    };

    let rendered = render_program(&program);
    assert!(
        rendered.contains("AI-NATIVE: read-only"),
        "AI-native expression should be marked read-only\nRendered:\n{}",
        rendered
    );
}

#[test]
fn render_ai_native_statement() {
    let program = crush_cast::Program {
        cast_version: "1.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        functions: {
            let mut m = std::collections::HashMap::new();
            m.insert(
                "main".to_string(),
                crush_cast::Function {
                    params: vec![],
                    body: vec![crush_cast::Statement::AI(
                        crush_cast::ai::AIStatement::GoalDeclaration {
                            goal_id: "g1".to_string(),
                            description: "test goal".to_string(),
                            success_criteria: vec!["done".to_string()],
                            priority: crush_cast::ai::Priority::High,
                            deadline: None,
                        },
                    )],
                    meta: std::collections::HashMap::new(),
                },
            );
            m
        },
        ai_meta: None,
    };

    let rendered = render_program(&program);
    assert!(
        rendered.contains("AI-NATIVE: read-only"),
        "AI-native statement should be marked read-only\nRendered:\n{}",
        rendered
    );
}
