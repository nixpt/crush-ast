use crush_frontend::parse_source;

/// CRUSH-74 DoD #1: a REAL parse must produce nodes carrying source
/// locations. The old `meta_at(line,col,file)` test helper fabricated meta
/// that no real parse produced — these tests exercise the actual path.
#[test]
fn real_parse_stamps_function_and_statement_locations() {
    let prog = parse_source("fn main() {\n    let x = 1;\n}\n").expect("parse should succeed");

    let func = prog.functions.get("main").expect("main function exists");
    assert_eq!(
        func.meta.get("line").and_then(|v| v.as_u64()),
        Some(1),
        "function node must carry the line of `fn`"
    );
    assert_eq!(
        func.meta.get("col").and_then(|v| v.as_u64()),
        Some(1),
        "function node must carry the col of `fn`"
    );

    let body = &func.body;
    assert_eq!(body.len(), 1, "expected exactly one statement");
    match &body[0] {
        crush_cast::Statement::VarDecl { meta, .. } => {
            assert_eq!(
                meta.get("line").and_then(|v| v.as_u64()),
                Some(2),
                "let statement on line 2 must carry line 2"
            );
        }
        other => panic!("expected VarDecl, got {other:?}"),
    }
}

/// Line numbers must track the source: a statement on line 5 must carry
/// line 5, not line 1 — catches a stamp-at-boot vs stamp-at-token bug.
#[test]
fn locations_track_source_lines() {
    let prog = parse_source("fn a() {\n    let x = 1;\n}\n\nfn b() {\n    let y = 2;\n}\n")
        .expect("parse should succeed");

    let func_b = prog.functions.get("b").expect("b function exists");
    assert_eq!(
        func_b.meta.get("line").and_then(|v| v.as_u64()),
        Some(5),
        "function b starts on line 5"
    );

    let body_b = &func_b.body;
    match &body_b[0] {
        crush_cast::Statement::VarDecl { meta, .. } => {
            assert_eq!(
                meta.get("line").and_then(|v| v.as_u64()),
                Some(6),
                "b's let statement is on line 6"
            );
        }
        other => panic!("expected VarDecl, got {other:?}"),
    }
}

// --- CRUSH-315: semantic errors carry a position ---------------------------

fn semantic_pos(src: &str) -> (Option<(usize, usize)>, String) {
    let program = crush_frontend::parse_source(src).expect("parses");
    let err = crush_frontend::lower_and_check(program).expect_err("must fail the checker");
    let sem = err
        .downcast_ref::<crush_frontend::SemanticError>()
        .expect("lower_and_check errors are SemanticError");
    (sem.pos, err.to_string())
}

#[test]
fn undefined_variable_is_located_at_the_identifier() {
    let (pos, text) = semantic_pos("let a = 1\nprint(undefined_name)\n");
    assert_eq!(pos, Some((2, 7)), "line 2, the column of `undefined_name`");
    // The text is unchanged by the position.
    assert_eq!(text, "Undefined variable: undefined_name");
}

#[test]
fn undefined_function_is_located_at_the_call() {
    let (pos, _) = semantic_pos("let a = 1\nundefined_fn(a)\n");
    assert_eq!(pos, Some((2, 1)));
}

#[test]
fn error_inside_a_function_body_names_its_own_line() {
    let (pos, _) = semantic_pos("fn f() {\n  let q = 1\n  return missing_var + q\n}\nprint(f())\n");
    assert_eq!(pos, Some((3, 10)));
}

#[test]
fn type_error_is_located_at_the_operator() {
    let (pos, text) = semantic_pos("let a = 1\nlet b = a - \"x\"\n");
    assert_eq!(pos, Some((2, 11)));
    assert!(text.contains("Invalid binary op"), "{text}");
}

#[test]
fn syntax_errors_stay_structured_through_parse_source() {
    let err = crush_frontend::parse_source("let b = (2 +\n").expect_err("syntax error");
    let parse = err
        .downcast_ref::<crush_frontend::ParseErrors>()
        .expect("parse_source keeps ParseErrors");
    assert!(!parse.0.is_empty());
    assert!(err.to_string().starts_with("Parse errors: "), "{err}");
}
