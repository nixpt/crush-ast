use crush_cast::{
    CastType, DomMutationType, DomQueryType, Expression, ExternalResourceType, Function,
    ImportStatement, Program, Statement, MatchArm, Pattern,
};
use crush_cast::manifest::{Invariant, ModuleManifest};
use crush_frontend::compiler::Compiler;
use std::collections::HashMap;

fn meta() -> HashMap<String, serde_json::Value> {
    HashMap::new()
}

fn meta_at(line: u32, col: u32) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    m.insert("line".to_string(), serde_json::json!(line));
    m.insert("col".to_string(), serde_json::json!(col));
    m.insert("file".to_string(), serde_json::json!("main.crush"));
    m
}

fn int(value: i64) -> Expression {
    Expression::IntLiteral {
        value,
        meta: meta(),
    }
}

fn bool_lit(value: bool) -> Expression {
    Expression::BoolLiteral {
        value,
        meta: meta(),
    }
}

fn string(value: &str) -> Expression {
    Expression::StringLiteral {
        value: value.to_string(),
        meta: meta(),
    }
}

fn var(name: &str) -> Expression {
    Expression::Var {
        name: name.to_string(),
        meta: meta(),
    }
}

fn create_program(body: Vec<Statement>) -> Program {
    let mut functions = HashMap::new();
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            body,
            meta: meta(),
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

fn compile_program(body: Vec<Statement>) -> casm::Program {
    let mut compiler = Compiler::new();
    compiler
        .compile(create_program(body))
        .expect("compilation should succeed")
}

fn compile_program_with_debug(body: Vec<Statement>) -> (casm::Program, casm::DebugInfo) {
    let mut compiler = Compiler::new();
    let program = compiler
        .compile(create_program(body))
        .expect("compilation should succeed");
    let debug = compiler
        .debug_info()
        .cloned()
        .expect("debug info should be available");
    (program, debug)
}

fn main_ops(casm: &casm::Program) -> Vec<&str> {
    casm.functions
        .get("main")
        .expect("main function missing")
        .body
        .iter()
        .map(|i| i.op.as_str())
        .collect()
}

#[test]
fn source_map_length_matches_instruction_count() {
    let body = vec![Statement::VarDecl {
        name: "x".to_string(),
        value: Expression::IntLiteral {
            value: 10,
            meta: meta_at(1, 5),
        },
        type_hint: CastType::Any,
        meta: meta_at(1, 1),
    }];
    let (casm, debug) = compile_program_with_debug(body);
    let count: usize = casm.functions.values().map(|f| f.body.len()).sum();
    assert_eq!(debug.source_map.len(), count);
}

#[test]
fn source_map_records_line_col_for_key_instructions() {
    let body = vec![Statement::Return {
        value: Some(Expression::Call {
            function: "foo".to_string(),
            args: vec![Expression::IntLiteral {
                value: 1,
                meta: meta_at(3, 14),
            }],
            meta: meta_at(3, 9),
        }),
        meta: meta_at(3, 1),
    }];
    let (_casm, debug) = compile_program_with_debug(body);

    assert!(!debug.source_map.is_empty());
    assert!(
        debug
            .source_map
            .iter()
            .any(|loc| loc.line == 3 && loc.col == 14)
    ); // push arg
    assert!(
        debug
            .source_map
            .iter()
            .any(|loc| loc.line == 3 && loc.col == 9)
    ); // call
    assert!(
        debug
            .source_map
            .iter()
            .any(|loc| loc.line == 3 && loc.col == 1)
    ); // ret
}

#[test]
fn source_map_preserves_multiple_source_lines() {
    let body = vec![
        Statement::VarDecl {
            name: "a".to_string(),
            value: Expression::IntLiteral {
                value: 1,
                meta: meta_at(1, 9),
            },
            type_hint: CastType::Any,
            meta: meta_at(1, 1),
        },
        Statement::VarDecl {
            name: "b".to_string(),
            value: Expression::IntLiteral {
                value: 2,
                meta: meta_at(2, 9),
            },
            type_hint: CastType::Any,
            meta: meta_at(2, 1),
        },
    ];
    let (_casm, debug) = compile_program_with_debug(body);
    assert!(debug.source_map.iter().any(|loc| loc.line == 1));
    assert!(debug.source_map.iter().any(|loc| loc.line == 2));
}

#[test]
fn compiles_var_decl_and_assignment_like_rebinds() {
    let casm = compile_program(vec![
        Statement::VarDecl {
            name: "x".to_string(),
            value: int(5),
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::VarDecl {
            name: "x".to_string(),
            value: int(10),
            type_hint: CastType::Any,
            meta: meta(),
        },
    ]);

    let main = casm.functions.get("main").unwrap();
    let x_stores = main
        .body
        .iter()
        .filter(|ins| ins.op == "store" && ins.args["name"] == "x")
        .count();
    assert_eq!(x_stores, 2);
}

#[test]
fn compiles_if_else_with_branching_jumps() {
    let casm = compile_program(vec![Statement::If {
        condition: bool_lit(true),
        then_body: vec![Statement::Return {
            value: Some(int(1)),
            meta: meta(),
        }],
        else_body: Some(vec![Statement::Return {
            value: Some(int(2)),
            meta: meta(),
        }]),
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"jmp_if_not"));
    assert!(ops.contains(&"jmp"));
}

#[test]
fn compiles_while_with_break_and_continue() {
    let casm = compile_program(vec![Statement::While {
        condition: Box::new(bool_lit(true)),
        body: vec![
            Statement::Continue { meta: meta() },
            Statement::Break { meta: meta() },
        ],
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"jmp_if_not"));
    assert!(ops.iter().filter(|op| **op == "jmp").count() >= 3);
}

#[test]
fn compiles_for_loop_desugaring() {
    let casm = compile_program(vec![Statement::For {
        variable: "item".to_string(),
        iterable: Box::new(Expression::ArrayLiteral {
            elements: vec![int(1), int(2)],
            meta: meta(),
        }),
        body: vec![Statement::ExprStmt {
            expr: var("item"),
            meta: meta(),
        }],
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"len"));
    assert!(ops.contains(&"index"));
}

#[test]
fn compiles_nested_function_definitions() {
    let casm = compile_program(vec![
        Statement::FunctionDef {
            name: "add".to_string(),
            params: vec![
                ("a".to_string(), CastType::Int),
                ("b".to_string(), CastType::Int),
            ],
            body: vec![Statement::Return {
                value: Some(Expression::BinaryOp {
                    operator: "+".to_string(),
                    left: Box::new(var("a")),
                    right: Box::new(var("b")),
                    meta: meta(),
                }),
                meta: meta(),
            }],
            meta: meta(),
        },
        Statement::ExprStmt {
            expr: Expression::Call {
                function: "add".to_string(),
                args: vec![int(1), int(2)],
                meta: meta(),
            },
            meta: meta(),
        },
    ]);

    assert!(casm.functions.contains_key("add"));
    let main = casm.functions.get("main").unwrap();
    assert!(
        main.body
            .iter()
            .any(|ins| ins.op == "call" && ins.args["function"] == "add")
    );
}

#[test]
fn compiles_struct_field_access_and_mutation() {
    let casm = compile_program(vec![
        Statement::VarDecl {
            name: "u".to_string(),
            value: Expression::NewStruct {
                name: "User".to_string(),
                meta: meta(),
            },
            type_hint: CastType::TypeRef("User".to_string()),
            meta: meta(),
        },
        Statement::SetField {
            target: var("u"),
            field: "name".to_string(),
            value: string("alice"),
            meta: meta(),
        },
        Statement::ExprStmt {
            expr: Expression::GetField {
                target: Box::new(var("u")),
                field: "name".to_string(),
                meta: meta(),
            },
            meta: meta(),
        },
    ]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"new_struct"));
    assert!(ops.contains(&"set_field"));
    assert!(ops.contains(&"get_field"));
}

#[test]
fn compiles_array_object_and_index_expressions() {
    let casm = compile_program(vec![
        Statement::VarDecl {
            name: "arr".to_string(),
            value: Expression::ArrayLiteral {
                elements: vec![int(7), int(8), int(9)],
                meta: meta(),
            },
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::VarDecl {
            name: "obj".to_string(),
            value: Expression::ObjectLiteral {
                properties: vec![("x".to_string(), int(1)), ("y".to_string(), int(2))],
                meta: meta(),
            },
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::ExprStmt {
            expr: Expression::Index {
                target: Box::new(var("arr")),
                index: Box::new(int(1)),
                meta: meta(),
            },
            meta: meta(),
        },
    ]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"new_array"));
    assert!(ops.contains(&"new_obj"));
    assert!(ops.contains(&"index"));
}

#[test]
fn compiles_try_catch_and_throw() {
    let casm = compile_program(vec![Statement::TryCatch {
        body: vec![Statement::Throw {
            value: string("boom"),
            meta: meta(),
        }],
        error_var: "err".to_string(),
        handler: vec![Statement::ExprStmt {
            expr: var("err"),
            meta: meta(),
        }],
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"enter_try"));
    assert!(ops.contains(&"exit_try"));
    assert!(ops.contains(&"throw"));
}

#[test]
fn compiles_capability_calls_and_collects_manifest_permissions() {
    let casm = compile_program(vec![Statement::ExprStmt {
        expr: Expression::CapabilityCall {
            name: "net.fetch".to_string(),
            args: vec![string("https://example.com")],
            meta: meta(),
        },
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"cap_call"));
    assert!(
        casm.manifest
            .permissions
            .iter()
            .any(|perm| perm == "net.fetch")
    );
}

#[test]
fn builtin_len_capability_call_stays_len_and_no_permission() {
    let casm = compile_program(vec![Statement::ExprStmt {
        expr: Expression::CapabilityCall {
            name: "len".to_string(),
            args: vec![Expression::ArrayLiteral {
                elements: vec![int(1), int(2)],
                meta: meta(),
            }],
            meta: meta(),
        },
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"len"));
    assert!(casm.manifest.permissions.is_empty());
}

#[test]
fn compiles_polyglot_lang_block_with_variable_mapping() {
    let casm = compile_program(vec![
        Statement::VarDecl {
            name: "x".to_string(),
            value: int(42),
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::LangBlock {
            lang: "python".to_string(),
            code: "result = x + 1".to_string(),
            variables: vec!["x".to_string()],
            imports: vec![],
            meta: meta(),
        },
    ]);

    let main = casm.functions.get("main").unwrap();
    let exec = main
        .body
        .iter()
        .find(|ins| ins.op == "exec_lang")
        .expect("exec_lang missing");
    assert_eq!(exec.args["lang"], "python");
    assert_eq!(exec.args["var_count"], 1);
    assert_eq!(exec.args["var_0"], "x");
}

#[test]
fn import_statement_is_accepted_by_compiler() {
    let casm = compile_program(vec![Statement::Import {
        import: ImportStatement::External {
            uri: "https://example.com/data.json".to_string(),
            resource_type: ExternalResourceType::Http,
            alias: Some("data".to_string()),
        },
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    // External imports lower to: push uri, external.load cap_call, store alias.
    assert_eq!(
        ops,
        vec!["push_str", "cap_call", "store", "push_null", "ret"]
    );

    let main = casm.functions.get("main").unwrap();
    let cap = main
        .body
        .iter()
        .find(|ins| ins.op == "cap_call")
        .expect("cap_call missing");
    assert_eq!(cap.args["name"], "external.load");
}

#[test]
fn compiles_dom_query_mutate_and_event_listener() {
    let casm = compile_program(vec![
        Statement::VarDecl {
            name: "el".to_string(),
            value: Expression::DomQuery {
                query_type: DomQueryType::GetElementById,
                selector: Box::new(string("status")),
                meta: meta(),
            },
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::DomMutate {
            target: var("el"),
            mutation_type: DomMutationType::SetTextContent,
            value: Some(string("ready")),
            value2: None,
            meta: meta(),
        },
        Statement::DomEventListener {
            target: var("el"),
            event: "click".to_string(),
            callback: var("on_click"),
            meta: meta(),
        },
    ]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"dom_query"));
    assert!(ops.contains(&"dom_mutate"));
    assert!(ops.contains(&"dom_event_listener"));
}

#[test]
fn compiles_pipeline_call_with_implicit_first_argument() {
    let casm = compile_program(vec![Statement::ExprStmt {
        expr: Expression::Pipeline {
            segments: vec![
                int(5),
                Expression::Call {
                    function: "double".to_string(),
                    args: vec![],
                    meta: meta(),
                },
            ],
            meta: meta(),
        },
        meta: meta(),
    }]);

    let main = casm.functions.get("main").unwrap();
    let call = main
        .body
        .iter()
        .find(|ins| ins.op == "call")
        .expect("call missing");
    assert_eq!(call.args["function"], "double");
    assert_eq!(call.args["argc"], 1);
}

#[test]
fn compiles_common_builtin_calls() {
    let casm = compile_program(vec![
        Statement::ExprStmt {
            expr: Expression::Call {
                function: "str.contains".to_string(),
                args: vec![string("hello"), string("ell")],
                meta: meta(),
            },
            meta: meta(),
        },
        Statement::ExprStmt {
            expr: Expression::Call {
                function: "array.push".to_string(),
                args: vec![
                    Expression::ArrayLiteral {
                        elements: vec![int(1)],
                        meta: meta(),
                    },
                    int(2),
                ],
                meta: meta(),
            },
            meta: meta(),
        },
        Statement::ExprStmt {
            expr: Expression::Call {
                function: "array.pop".to_string(),
                args: vec![Expression::ArrayLiteral {
                    elements: vec![int(1)],
                    meta: meta(),
                }],
                meta: meta(),
            },
            meta: meta(),
        },
    ]);

    let ops = main_ops(&casm);
    assert!(ops.contains(&"str_contains"));
    assert!(ops.contains(&"array_push"));
    assert!(ops.contains(&"array_pop"));
}

#[test]
fn empty_program_main_gets_synthetic_return() {
    let casm = compile_program(vec![]);
    let ops = main_ops(&casm);
    assert_eq!(ops, vec!["push_null", "ret"]);
}

#[test]
fn deeply_nested_expression_compiles_without_stack_overflow() {
    let mut expr = int(0);
    for i in 1..=200 {
        expr = Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(expr),
            right: Box::new(int(i)),
            meta: meta(),
        };
    }

    let handle = std::thread::Builder::new()
        .name("deep-nesting-compile".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let casm = compile_program(vec![Statement::ExprStmt { expr, meta: meta() }]);
            let ops = main_ops(&casm);
            assert!(ops.contains(&"add"));
        })
        .expect("failed to spawn test thread");

    handle.join().expect("deep nesting compile thread panicked");
}

#[test]
fn very_long_string_literal_compiles() {
    let value = "x".repeat(10 * 1024);
    let casm = compile_program(vec![Statement::Return {
        value: Some(Expression::StringLiteral {
            value: value.clone(),
            meta: meta(),
        }),
        meta: meta(),
    }]);

    let main = casm.functions.get("main").unwrap();
    let push = main
        .body
        .iter()
        .find(|ins| ins.op == "push_str")
        .expect("push_str missing");
    assert_eq!(push.args["value"].as_str().unwrap().len(), value.len());
}

#[test]
fn break_outside_loop_is_rejected() {
    let mut compiler = Compiler::new();
    let result = compiler.compile(create_program(vec![Statement::Break { meta: meta() }]));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("break outside of loop")
    );
}

#[test]
fn continue_outside_loop_is_rejected() {
    let mut compiler = Compiler::new();
    let result = compiler.compile(create_program(vec![Statement::Continue { meta: meta() }]));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("continue outside of loop")
    );
}

#[test]
fn modulo_operator_compiles_to_mod_op() {
    // `%` compiles to nanovm "mod" opcode (which the interpreter and fastvm both handle).
    let casm = compile_program(vec![Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "%".to_string(),
            left: Box::new(int(7)),
            right: Box::new(int(3)),
            meta: meta(),
        },
        meta: meta(),
    }]);
    let ops = main_ops(&casm);
    assert!(ops.contains(&"mod"), "expected mod op in {ops:?}");
}

#[test]
fn unsupported_operator_is_rejected() {
    // Regression guard for the compiler's unknown-operator path. `^` (bitwise XOR)
    // is parsed but not lowered to any opcode.
    let mut compiler = Compiler::new();
    let result = compiler.compile(create_program(vec![Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "^".to_string(),
            left: Box::new(int(4)),
            right: Box::new(int(2)),
            meta: meta(),
        },
        meta: meta(),
    }]));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unsupported op"));
}

#[test]
fn lambda_and_match_compile_successfully() {
    let mut compiler = Compiler::new();

    let lambda_result = compiler.compile(create_program(vec![Statement::ExprStmt {
        expr: Expression::Lambda {
            params: vec![("x".to_string(), CastType::Int)],
            body: vec![],
            meta: meta(),
        },
        meta: meta(),
    }]));
    assert!(lambda_result.is_ok());

    let match_result = compiler.compile(create_program(vec![Statement::ExprStmt {
        expr: Expression::Match {
            expression: Box::new(int(1)),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard,
                body: vec![],
            }],
            meta: meta(),
        },
        meta: meta(),
    }]));
    assert!(match_result.is_ok());
}

#[test]
fn spawn_with_args_is_rejected() {
    let mut compiler = Compiler::new();
    let result = compiler.compile(create_program(vec![Statement::ExprStmt {
        expr: Expression::Spawn {
            function: "worker".to_string(),
            args: vec![int(1)],
            meta: meta(),
        },
        meta: meta(),
    }]));

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("spawn does not currently support arguments")
    );
}

#[test]
fn compiles_function_with_maximum_arity() {
    // Test function with 255 parameters (maximum arity)
    let mut params = Vec::new();
    for i in 0..255 {
        params.push((format!("p{}", i), CastType::Any));
    }

    let casm = compile_program(vec![Statement::FunctionDef {
        name: "many_args".to_string(),
        params,
        body: vec![Statement::Return {
            value: Some(int(42)),
            meta: meta(),
        }],
        meta: meta(),
    }]);

    assert!(casm.functions.contains_key("many_args"));
    let func = casm.functions.get("many_args").unwrap();
    assert_eq!(func.params.len(), 255);
}

#[test]
fn compiles_nested_scopes_with_shadowing() {
    // Test that variables in inner scopes can shadow outer scope variables
    let casm = compile_program(vec![
        Statement::VarDecl {
            name: "x".to_string(),
            value: int(1),
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::VarDecl {
            name: "x".to_string(),
            value: int(2),
            type_hint: CastType::Any,
            meta: meta(),
        },
        Statement::Return {
            value: Some(var("x")),
            meta: meta(),
        },
    ]);

    // Should compile - second declaration shadows first
    let ops = main_ops(&casm);
    // Should have two store instructions
    let store_count = ops.iter().filter(|op| *op == &"store").count();
    assert_eq!(store_count, 2);
}

#[test]
fn compiles_empty_block_in_else_branch() {
    // Test that empty blocks compile correctly
    let casm = compile_program(vec![Statement::If {
        condition: bool_lit(false),
        then_body: vec![Statement::Return {
            value: Some(int(1)),
            meta: meta(),
        }],
        else_body: Some(vec![]), // Empty else block
        meta: meta(),
    }]);

    let ops = main_ops(&casm);
    // Should still have jmp_if_not to skip then_body
    assert!(ops.contains(&"jmp_if_not"));
}

#[test]
fn compiles_multiple_comments_in_source() {
    // Test multiple single-line comments compile
    let source = " // comment 1\n let x = 1 // comment 2\n // comment 3\n let y = 2";

    // Parse and compile through the full pipeline
    let result = crush_frontend::compile_crush_source(source);
    assert!(
        result.is_ok(),
        "multiple comments should compile: {:?}",
        result.err()
    );
}

#[test]
fn compiles_trailing_comma_in_array() {
    // Test trailing comma in array literal compiles
    let source = "let arr = [1, 2, 3,]";
    let result = crush_frontend::compile_crush_source(source);
    assert!(
        result.is_ok(),
        "trailing comma should compile: {:?}",
        result.err()
    );
}

#[test]
fn compiles_whitespace_only_program() {
    // Test that whitespace-only source compiles
    let source = "   \n\n   \t  \n";
    let result = crush_frontend::compile_crush_source(source);
    assert!(
        result.is_ok(),
        "whitespace only should compile: {:?}",
        result.err()
    );
}

/// Build a manifest with a single invariant that targets `func_name` and
/// carries the given `check_source`. Invariant.name is derived from
/// `func_name` so two tests can coexist without shared-state collisions.
fn manifest_for_function(func_name: &str, check_source: &str) -> ModuleManifest {
    ModuleManifest {
        purpose: "test".to_string(),
        invariants: vec![Invariant {
            name: format!("{func_name}_invariant"),
            description: "test".to_string(),
            applies_to: vec![func_name.to_string()],
            consequence: None,
            check_source: Some(check_source.to_string()),
        }],
        ..Default::default()
    }
}

/// When `with_invariant_runtime(true)` is set, every matching `@invariant`
/// gets a `cap_call "invariant.evaluate"` instruction after the function's
/// param-store loop, with structured args carrying invariant_name,
/// function_name, and check_source. Non-matching invariants do NOT emit,
/// and the cap_call permission is registered in `casm.manifest.permissions`.
#[test]
fn emits_runtime_invariant_cap_calls_when_flag_enabled() {
    let mut program = create_program(vec![Statement::VarDecl {
        name: "x".to_string(),
        value: int(7),
        type_hint: CastType::Any,
        meta: meta(),
    }]);
    program.manifest = Some(ModuleManifest {
        purpose: "test".to_string(),
        invariants: vec![
            Invariant {
                name: "matching".to_string(),
                description: "applies to main".to_string(),
                applies_to: vec!["main".to_string()],
                consequence: None,
                check_source: Some("ctx > 0".to_string()),
            },
            Invariant {
                name: "non_matching".to_string(),
                description: "targets a different function".to_string(),
                applies_to: vec!["unrelated_fn".to_string()],
                consequence: None,
                check_source: Some("never_runs".to_string()),
            },
            Invariant {
                // An invariant that *targets main* but lacks a check_source
                // should be silently skipped (consistent with `wip_check.rs`
                // silent-skip on missing data) — guards against the cap_call
                // cap from being requested accidentally on design-only docs.
                name: "design_only".to_string(),
                description: "doc-only invariant, no check_source".to_string(),
                applies_to: vec!["main".to_string()],
                consequence: Some("if violated → data corruption".to_string()),
                check_source: None,
            },
        ],
        ..Default::default()
    });

    let mut compiler = Compiler::new().with_invariant_runtime(true);
    let casm = compiler.compile(program).expect("compile");
    let main_body = &casm
        .functions
        .get("main")
        .expect("main function missing")
        .body;

    // Exactly one cap_call produced: the matching invariant.
    let cap_calls: Vec<_> = main_body
        .iter()
        .filter(|ins| ins.op == "cap_call" && ins.args["name"] == "invariant.evaluate")
        .collect();
    assert_eq!(
        cap_calls.len(),
        1,
        "expected exactly one invariant cap_call, got {cap_calls:?}"
    );

    let cap = cap_calls[0];
    assert_eq!(cap.args["invariant_name"], "matching");
    assert_eq!(cap.args["function_name"], "main");
    assert_eq!(cap.args["check_source"], "ctx > 0");
    assert_eq!(cap.args["argc"], 0);

    // Non-matching invariant must NOT have produced a cap_call.
    let non_matching_present = main_body
        .iter()
        .any(|ins| ins.args.get("invariant_name").and_then(|v| v.as_str()) == Some("non_matching"));
    assert!(
        !non_matching_present,
        "invariant targeting a different function must not emit"
    );

    // Design-only invariant (no check_source) must also NOT have emitted.
    let design_only_present = main_body
        .iter()
        .any(|ins| ins.args.get("invariant_name").and_then(|v| v.as_str()) == Some("design_only"));
    assert!(
        !design_only_present,
        "invariant without check_source must be skipped silently"
    );

    // Permission was registered in casm.manifest.
    assert!(
        casm.manifest.permissions.iter().any(|p| p == "invariant.evaluate"),
        "invariant.evaluate must appear in casm.manifest.permissions"
    );
}

/// Default behaviour (no builder call) must NOT emit any cap_call for
/// matching invariants, and must NOT register the cap permission.
#[test]
fn invariant_runtime_is_disabled_by_default() {
    let mut program = create_program(vec![Statement::Return {
        value: Some(int(0)),
        meta: meta(),
    }]);
    program.manifest = Some(manifest_for_function("main", "true"));

    // Use the explicit `Compiler::new().compile(program)` shape because the
    // `compile_program` helper at the top of this file takes `Vec<Statement>`
    // (it constructs a fresh `Program` from the body and loses our manifest
    // attachment). The explicit form preserves the manifest so this test
    // actually exercises the default-off path with a matching invariant.
    let mut compiler = Compiler::new();
    let casm = compiler.compile(program).expect("compile");
    let main_body = &casm
        .functions
        .get("main")
        .expect("main function missing")
        .body;

    assert!(
        !main_body
            .iter()
            .any(|ins| ins.op == "cap_call" && ins.args["name"] == "invariant.evaluate"),
        "no invariant cap_calls should be emitted by default"
    );
    assert!(
        !casm.manifest.permissions.iter().any(|p| p == "invariant.evaluate"),
        "no invariant.evaluate permission without flag"
    );
}

/// Mirrors the matching-invariant emit block: an `@invariant` whose
/// `check_source` is `Some("")` is effectively a doc stub — no evaluator can
/// run an empty expression. Confirm such stubs are silently skipped.
#[test]
fn empty_check_source_invariance_is_silently_skipped() {
    let mut program = create_program(vec![Statement::Return {
        value: Some(int(0)),
        meta: meta(),
    }]);
    program.manifest = Some(manifest_for_function("main", ""));

    let mut compiler = Compiler::new().with_invariant_runtime(true);
    let casm = compiler.compile(program).expect("compile");
    let main_body = &casm
        .functions
        .get("main")
        .expect("main function missing")
        .body;

    assert!(
        !main_body
            .iter()
            .any(|ins| ins.op == "cap_call" && ins.args["name"] == "invariant.evaluate"),
        "empty check_source should be silently skipped, not emitted"
    );
    assert!(
        !casm.manifest.permissions.iter().any(|p| p == "invariant.evaluate"),
        "no permission registered when check_source is empty"
    );
}
