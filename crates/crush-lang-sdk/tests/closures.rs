//! CRUSH-251 step 3: closures capture enclosing variables. Capture is by
//! reference (a captured variable lives in a cell), except a parameter that is
//! never reassigned, which is copied. Every program runs on the scheduler and
//! the portable VM, which must agree.

use crush_vm::portable_vm::PortableVm;
use crush_vm::{Quotas, run};

fn both(source: &str) -> Result<String, String> {
    let casm = crush_frontend::compile_crush_source(source).map_err(|e| format!("compile: {e:#}"))?;
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let native = run(&program, &Quotas::default()).map(|r| r.output).map_err(|e| e.to_string());
    let portable = PortableVm::new(program).run().map(|r| r.output).map_err(|e| e.to_string());
    assert_eq!(portable, native, "portable VM and scheduler disagree");
    native
}

fn ok(source: &str) -> String {
    both(source).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn adder_factory_copies_a_parameter() {
    assert_eq!(
        ok("fn make_adder(a) {\n  return |b| a + b\n}\nfn main() {\n  let add3 = make_adder(3)\n  let add10 = make_adder(10)\n  print(add3(4))\n  print(add10(4))\n  print(add3(1))\n}\n"),
        "7\n14\n4\n"
    );
}

#[test]
fn a_counter_mutates_its_captured_variable() {
    assert_eq!(
        ok("fn make_counter() {\n  let c = 0\n  return || {\n    c = c + 1\n    return c\n  }\n}\nfn main() {\n  let a = make_counter()\n  let b = make_counter()\n  print(a())\n  print(a())\n  print(b())\n  print(a())\n}\n"),
        "1\n2\n1\n3\n"
    );
}

#[test]
fn capture_is_by_reference_both_ways() {
    // The closure sees a later write by the function, and the function sees
    // the closure's write.
    assert_eq!(
        ok("fn main() {\n  let x = 1\n  let get = || x\n  let set = |v| {\n    x = v\n  }\n  x = 5\n  print(get())\n  set(9)\n  print(x)\n  print(get())\n}\n"),
        "5\n9\n9\n"
    );
}

#[test]
fn transitive_capture_through_a_middle_lambda() {
    assert_eq!(
        ok("fn make(a) {\n  return |b| |c| a + b + c\n}\nfn main() {\n  let f = make(100)\n  let g = f(20)\n  print(g(3))\n}\n"),
        "123\n"
    );
}

#[test]
fn closures_sharing_one_variable() {
    assert_eq!(
        ok("fn main() {\n  let n = 0\n  let inc = || {\n    n = n + 1\n  }\n  let get = || n\n  inc()\n  inc()\n  inc()\n  print(get())\n}\n"),
        "3\n"
    );
}

#[test]
fn recursion_through_a_captured_variable() {
    // `fact` is reassigned, so it isn't a direct-call alias: the lambda
    // captures the variable `fact` (boxed, since it is assigned after the
    // closure is made) and calls through it.
    assert_eq!(
        ok("fn main() {\n  let fact = 0\n  fact = |n| {\n    if n <= 1 {\n      return 1\n    }\n    return n * fact(n - 1)\n  }\n  print(fact(6))\n}\n"),
        "720\n"
    );
}

#[test]
fn a_captured_value_used_from_a_container() {
    assert_eq!(
        ok("fn main() {\n  let k = 3\n  let fs = [|x| x * k, |x| x + k]\n  let m = fs[0]\n  let a = fs[1]\n  print(m(5))\n  print(a(5))\n  k = 10\n  print(m(5))\n}\n"),
        "15\n8\n50\n"
    );
}

#[test]
fn closures_made_in_a_loop_share_the_loop_variable() {
    // Python-style late binding: one variable, one cell — every closure sees
    // its final value. (JS `for (let ..)` per-iteration bindings are the JS
    // walker's job, CRUSH-251 step 4.)
    assert_eq!(
        ok("fn main() {\n  let fs = []\n  let i = 0\n  while i < 3 {\n    fs.push(|| i)\n    i = i + 1\n  }\n  let f0 = fs[0]\n  let f2 = fs[2]\n  print(f0())\n  print(f2())\n}\n"),
        "3\n3\n"
    );
}

#[test]
fn a_closure_capturing_a_parameter_that_is_reassigned_sees_the_write() {
    assert_eq!(
        ok("fn f(n) {\n  let show = || n\n  n = n * 2\n  return show()\n}\nfn main() {\n  print(f(21))\n}\n"),
        "42\n"
    );
}

#[test]
fn a_lambda_without_captures_keeps_the_direct_call() {
    let casm = crush_frontend::compile_crush_source("fn main() {\n  let sq = |x| x * x\n  print(sq(4))\n}\n").unwrap();
    let main = &casm.functions["main"];
    assert!(main.body.iter().any(|i| i.op == "call"), "expected a direct call");
    assert!(!main.body.iter().any(|i| i.op == "call_value" || i.op.contains("cell")));
}

// A `let` is a fresh binding each time it runs: closures made in different
// loop iterations each keep their own `j` (JS / Rust `let` semantics). One
// binding shared across iterations gave `22`.
#[test]
fn a_let_in_a_loop_body_is_a_fresh_binding_per_iteration() {
    assert_eq!(
        ok("fn main() {\n  let fs = []\n  let i = 0\n  while i < 3 {\n    let j = i\n    fs.push(|| j)\n    i = i + 1\n  }\n  let f0 = fs[0]\n  let f2 = fs[2]\n  print(f0() * 10 + f2())\n}\n"),
        "2\n"
    );
}

#[test]
fn a_shadowing_let_is_a_new_binding() {
    assert_eq!(
        ok("fn main() {\n  let x = 1\n  let get = || x\n  let x = 2\n  print(get())\n  print(x)\n}\n"),
        "1\n2\n"
    );
}

#[test]
fn a_let_bound_lambda_that_refers_to_itself_and_captures() {
    assert_eq!(
        ok("fn main() {\n  let base = 1\n  let fact = |n| {\n    if n <= 1 {\n      return base\n    }\n    return n * fact(n - 1)\n  }\n  print(fact(5))\n}\n"),
        "120\n"
    );
}

// `xs.push(..)` inside a lambda on a captured `xs`: the parser emits a
// method call as a CapabilityCall, whose receiver wasn't seen as a capture
// ("unknown capability: xs.push").
#[test]
fn a_method_call_on_a_captured_variable() {
    assert_eq!(
        ok("fn main() {\n  let xs = []\n  let add = |v| {\n    xs.push(v)\n  }\n  add(1)\n  add(2)\n  print(xs)\n}\n"),
        "[1, 2]\n"
    );
}
