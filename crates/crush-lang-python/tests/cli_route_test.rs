//! CRUSH-377: the CLI route (`python_to_cast`, then the semantic checker and
//! compiler `crush-run` uses) must agree with CPython, or refuse loudly.
//! `cpython_corpus_test.rs` covers the other route, which skips the checker.

use crush_lang_sdk::compile::{CompileOptions, compile_cast_program};

fn run_cli_route(source: &str) -> anyhow::Result<String> {
    let cast = crush_lang_python::python_to_cast(source)?;
    let vm = compile_cast_program(cast, &CompileOptions::default())?;
    Ok(crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)?.output)
}

#[test]
fn and_or_on_comparisons_match_cpython() {
    let out = run_cli_route(
        "x = 5\n\
         print(x > 0 and x < 9)\n\
         print(x > 9 or x < 9)\n\
         print(x > 9 and x < 9)\n\
         print(x > 9 or x < 3)\n",
    )
    .unwrap();
    assert_eq!(out, "True\nTrue\nFalse\nFalse\n");
}

#[test]
fn and_short_circuits() {
    let out = run_cli_route(
        "def later():\n    print(\"evaluated\")\n    return True\n\
         x = 5\n\
         print(x > 9 and later())\n",
    )
    .unwrap();
    assert_eq!(out, "False\n");
}

#[test]
fn power_matches_cpython() {
    let out = run_cli_route("print(2 ** 3)\nprint(2.5 ** 2)\n").unwrap();
    assert_eq!(out, "8\n6.25\n");
}

/// `or` yields an operand in Python, but lowers to a bool here. A value that
/// might not be a bool must be refused, never silently turned into one.
#[test]
fn or_with_a_value_of_unknown_type_is_not_silently_a_bool() {
    match run_cli_route("def f(a):\n    return a or 5\nprint(f(0))\n") {
        Ok(out) => assert_eq!(out, "5\n"),
        Err(e) => assert!(e.to_string().contains("Logical operator or"), "{e}"),
    }
}

#[test]
fn or_with_int_operands_is_refused() {
    let err = run_cli_route("print(0 or 5)\n").expect_err("must not print a bool");
    assert!(err.to_string().contains("Logical operator or"), "{err}");
}
