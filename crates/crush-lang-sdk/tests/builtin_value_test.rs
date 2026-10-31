//! #257: a built-in used as a value (`let g = len`) is a compile error that
//! names the built-in and shows the lambda that wraps it — it used to compile
//! and fail at run time with "load from uninitialised slot", then (later) to
//! report the misleading "Undefined variable: len".

use crush_vm::{Quotas, run};

#[test]
fn a_builtin_as_a_value_is_refused_with_a_wrapping_hint() {
    for (src, hint) in [
        ("let g = len\nprint(g([1]))\n", "|x| len(x)"),
        ("print(print)\n", "|x| print(x)"),
        ("fn main() {\n    let f = parallel_map\n    return 0\n}\n", "|x, y| parallel_map(x, y)"),
    ] {
        let err = crush_frontend::compile_crush_source(src).expect_err(src).to_string();
        assert!(err.contains("is a built-in, not a function value"), "{src}: {err}");
        assert!(err.contains(hint), "{src}: {err}");
    }
}

#[test]
fn the_suggested_lambda_works() {
    let casm = crush_frontend::compile_crush_source("let g = |x| len(x)\nprint(g([1, 2, 3]))\n").expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    assert_eq!(run(&program, &Quotas::default()).expect("run").output, "3\n");
}

#[test]
fn an_unknown_name_is_still_undefined() {
    let err = crush_frontend::compile_crush_source("let g = nosuch\n").expect_err("nosuch").to_string();
    assert!(err.contains("Undefined variable: nosuch"), "{err}");
}
