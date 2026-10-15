//! CRUSH-251 step 1: every lambda lifts to a unique function; a `let` binding
//! is a scoped alias. Lambdas used to be lifted *as* their binding name, so:
//! - two functions' `let h = ..` lambdas overwrote each other (which one won
//!   depended on lifting order: `11 11` or `1000 1000`);
//! - a `let f = ..` lambda replaced a top-level `fn f` program-wide.

use crush_vm::{Quotas, run};

fn run_source(source: &str) -> String {
    let casm = crush_frontend::compile_crush_source(source).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    run(&program, &Quotas::default()).expect("run").output
}

#[test]
fn same_named_lambdas_in_different_functions_stay_separate() {
    let out = run_source(
        "fn a() {\n  let h = |x| x + 1\n  return h(10)\n}\nfn b() {\n  let h = |x| x * 100\n  return h(10)\n}\nfn main() {\n  print(a())\n  print(b())\n}\n",
    );
    assert_eq!(out, "11\n1000\n");
}

#[test]
fn a_let_lambda_does_not_replace_a_top_level_function() {
    let out = run_source(
        "fn f(x) {\n  return x + 1000\n}\nfn g() {\n  let f = |x| x * 2\n  return f(5)\n}\nfn main() {\n  print(g())\n  print(f(1))\n}\n",
    );
    assert_eq!(out, "10\n1001\n");
}

#[test]
fn an_outer_lambda_is_callable_from_a_nested_lambda() {
    let out = run_source(
        "fn main() {\n  let sq = |x| x * x\n  let twice_sq = |y| sq(y) + sq(y)\n  print(twice_sq(3))\n}\n",
    );
    assert_eq!(out, "18\n");
}

#[test]
fn branch_local_lambdas_with_the_same_name() {
    let out = run_source(
        "fn pick(c) {\n  if c {\n    let f = |x| x + 1\n    return f(10)\n  } else {\n    let f = |x| x * 100\n    return f(10)\n  }\n}\nfn main() {\n  print(pick(true))\n  print(pick(false))\n}\n",
    );
    assert_eq!(out, "11\n1000\n");
}

#[test]
fn recursion_through_a_lambda_alias() {
    let out = run_source(
        "fn main() {\n  let fact = |n| {\n    if n <= 1 {\n      return 1\n    }\n    return n * fact(n - 1)\n  }\n  print(fact(5))\n}\n",
    );
    assert_eq!(out, "120\n");
}

fn compile_error(source: &str) -> String {
    format!("{:#}", crush_frontend::compile_crush_source(source).expect_err("must not compile"))
}

// Reassigning a lambda binding (in a loop, a branch, straight-line) and
// shadowing one in an inner scope were compile errors here until CRUSH-251
// step 2; they now run correctly — see `function_values.rs`.
#[test]
fn rebinding_to_a_non_lambda_drops_the_alias() {
    // `f` is 5 now; calling it must not silently call the old lambda.
    let e = compile_error(
        "fn main() {\n  let f = |x| x + 1\n  let r = f(1)\n  let f = 5\n  print(f(2))\n}\n",
    );
    assert!(e.contains("`f` holds a value of type int, not a function"), "{e}");
}
