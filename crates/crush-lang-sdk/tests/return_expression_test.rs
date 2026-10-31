//! #253: `return` followed by an expression that `parse_primary` starts with
//! `match`, `new` or `async.` parsed as a bare `return` and then an
//! unreachable statement, so the function silently returned null.

use crush_vm::{Quotas, run};

fn run_source(source: &str) -> String {
    let casm = crush_frontend::compile_crush_source(source).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    run(&program, &Quotas::default()).expect("run").output
}

#[test]
fn return_match_returns_the_matched_arm() {
    let out = run_source(
        "fn f(v) {\n    return match v { 1 => \"one\", _ => \"other\" }\n}\nprint(f(1))\nprint(f(2))\n",
    );
    assert_eq!(out, "one\nother\n");
}

#[test]
fn return_new_returns_the_instance() {
    // (`make()` is now typed `struct P`, so even `p == null` no longer
    // type-checks; use the instance instead)
    let out = run_source(
        "struct P { x: Int }\nfn make() {\n    return new P()\n}\nlet p = make()\np.x = 5\nprint(p.x)\n",
    );
    assert_eq!(out, "5\n");
}

#[test]
fn return_async_namespace_call_is_the_return_value() {
    let program = crush_frontend::parse_source("fn t() {\n    return async.sleep(1)\n}\n").expect("parse");
    let body = &program.functions["t"].body;
    assert_eq!(body.len(), 1, "the call must not be a separate statement: {body:?}");
    match &body[0] {
        crush_cast::Statement::Return { value, .. } => assert!(value.is_some(), "bare return"),
        other => panic!("expected a return, got {other:?}"),
    }
}
