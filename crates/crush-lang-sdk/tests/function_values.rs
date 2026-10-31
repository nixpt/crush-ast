//! CRUSH-251 step 2: function values. A lambda or a function's name used as a
//! value is a `Func` (`MAKE_CLOSURE`); calling a variable that holds one goes
//! through `CALL_VALUE`. Every program runs on both the scheduler and the
//! portable VM, which must agree — on the output and on the error.

use crush_vm::portable_vm::PortableVm;
use crush_vm::{Quotas, run};

fn both(source: &str) -> Result<String, String> {
    let casm = crush_frontend::compile_crush_source(source).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let native = run(&program, &Quotas::default()).map(|r| r.output).map_err(|e| e.to_string());
    let portable = PortableVm::new(program).run().map(|r| r.output).map_err(|e| e.to_string());
    assert_eq!(portable, native, "portable VM and scheduler disagree");
    native
}

fn ok(source: &str) -> String {
    both(source).expect("runs")
}

#[test]
fn a_lambda_passed_to_a_function() {
    assert_eq!(ok("fn apply(f, v) {\n  return f(v)\n}\nfn main() {\n  print(apply(|x| x * 2, 21))\n}\n"), "42\n");
}

#[test]
fn a_named_function_passed_as_a_value() {
    assert_eq!(
        ok("fn double(x) {\n  return x * 2\n}\nfn apply(f, v) {\n  return f(v)\n}\nfn main() {\n  print(apply(double, 21))\n  let g = double\n  print(g(5))\n}\n"),
        "42\n10\n"
    );
}

#[test]
fn a_function_returned_from_a_function() {
    assert_eq!(
        ok("fn inc(x) {\n  return x + 1\n}\nfn dbl(x) {\n  return x * 2\n}\nfn pick(n) {\n  if n == 1 {\n    return inc\n  }\n  return dbl\n}\nfn main() {\n  let f = pick(1)\n  let g = pick(2)\n  print(f(10))\n  print(g(10))\n}\n"),
        "11\n20\n"
    );
}

#[test]
fn functions_stored_in_arrays_and_maps() {
    assert_eq!(
        ok("fn inc(x) {\n  return x + 1\n}\nfn main() {\n  let fs = [inc, |x| x * 2]\n  let g = fs[1]\n  print(g(5))\n  let ops = {\"add\": |a, b| a + b}\n  let h = ops[\"add\"]\n  print(h(2, 3))\n}\n"),
        "10\n5\n"
    );
}

// Step 1 made these compile errors, because the alias was resolved in source
// order: the loop printed `110011` and the branch `1000`.
#[test]
fn reassigning_a_lambda_binding_in_a_loop() {
    assert_eq!(
        ok("fn main() {\n  let f = |x| x + 1\n  let i = 0\n  let out = 0\n  while i < 2 {\n    out = out * 10000 + f(10)\n    f = |x| x * 100\n    i = i + 1\n  }\n  print(out)\n}\n"),
        "111000\n"
    );
}

#[test]
fn conditionally_reassigning_a_lambda_binding() {
    assert_eq!(
        ok("fn g(c) {\n  let f = |x| x + 1\n  if c {\n    f = |x| x * 100\n  }\n  return f(10)\n}\nfn main() {\n  print(g(false))\n  print(g(true))\n}\n"),
        "11\n1000\n"
    );
}

#[test]
fn straight_line_reassignment() {
    assert_eq!(
        ok("fn main() {\n  let f = |x| x + 1\n  let a = f(10)\n  f = |x| x * 100\n  let b = f(10)\n  print(a * 10000 + b)\n}\n"),
        "111000\n"
    );
}

#[test]
fn a_parameter_shadowing_an_outer_lambda_holds_what_was_passed() {
    assert_eq!(
        ok("fn run() {\n  let f = |x| x + 1\n  let g = |f| f(1)\n  return g(|x| x * 7)\n}\nfn main() {\n  print(run())\n}\n"),
        "7\n"
    );
}

#[test]
fn calling_a_non_function() {
    let e = both("fn main() {\n  let x = 5\n  print(x(1))\n}\n").unwrap_err();
    assert!(e.contains("int is not callable"), "{e}");
    // An inner `let f = 5` shadows the outer lambda (it used to call it).
    let e = both("fn run() {\n  let f = |x| x + 1\n  let g = |y| {\n    let f = 5\n    return f(y)\n  }\n  return g(1)\n}\nfn main() {\n  print(run())\n}\n").unwrap_err();
    assert!(e.contains("int is not callable"), "{e}");
    // Only known at run time: a runtime error, the same on both engines.
    let e = both("fn call1(f) {\n  return f(1)\n}\nfn main() {\n  print(call1(5))\n}\n").unwrap_err();
    assert!(e.contains("int is not callable"), "{e}");
}

#[test]
fn calling_with_the_wrong_number_of_arguments_is_a_runtime_error() {
    let e = both("fn apply2(f) {\n  return f(1, 2)\n}\nfn main() {\n  print(apply2(|x| x))\n}\n").unwrap_err();
    assert!(e.contains("takes 1 argument(s), got 2"), "{e}");
}

#[test]
fn printing_a_function_value() {
    assert_eq!(
        ok("fn inc(x) {\n  return x + 1\n}\nfn main() {\n  let sq = |x| x * x\n  let fs = [inc]\n  print(inc)\n  print(fs[0])\n}\n"),
        "<fn inc>\n<fn inc>\n"
    );
}

// Was a compile error until CRUSH-251 step 5 (SPAWN takes a function value).
#[test]
fn spawning_a_function_variable() {
    assert_eq!(
        ok("fn w(x) {\n  return x * 2\n}\nfn main() {\n  let f = w\n  let h = spawn f(21)\n  yield\n  print(await h)\n}\n"),
        "42\n"
    );
}

// #192 lifted `let w = |..|` as `__lambda_N_w` but `spawn w(..)` still pushed
// the name "w"; the engines handed back a null handle for an unknown function,
// so this silently printed `null`.
#[test]
fn spawning_a_let_bound_lambda() {
    assert_eq!(
        ok("fn main() {\n  let w = |x| {\n    return x * 2\n  }\n  let h = spawn w(21)\n  yield\n  let r = await h\n  print(r)\n}\n"),
        "42\n"
    );
}
