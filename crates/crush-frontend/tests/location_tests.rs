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
