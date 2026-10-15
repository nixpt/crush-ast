//! #78: the documented lambda forms parse and run. Calling a lambda through a
//! variable works since CRUSH-251 step 2, capturing since step 3.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

/// The issue's repro: was `unexpected token in expression: =>`.
#[test]
fn documented_forms() {
    let out = run(r#"
fn main() {
  let adder = |x| => x + 10
  print(adder(5))
  let sq = |x| { return x * x }
  print(sq(4))
  let add = |a, b| a + b
  print(add(2, 3))
  let hi = || => "hi"
  print(hi())
  let typed = |x: Int| -> x * 3
  print(typed(2))
  print(true || false)
}
"#);
    assert_eq!(out, "15\n16\n5\nhi\n6\ntrue\n");
}

#[test]
fn lambda_locals_are_its_own() {
    let out = run(r#"
fn main() {
  let f = |n| { let acc = 0  let i = 0  while i < n { acc = acc + i  i = i + 1 }  return acc }
  print(f(5))
}
"#);
    assert_eq!(out, "10\n");
}

// Was a compile error ("lambdas can't capture") until CRUSH-251 step 3.
#[test]
fn a_lambda_captures_an_enclosing_variable() {
    let out = run("fn main() {\n  let k = 3\n  let f = |x| { return x + k }\n  print(f(1))\n}");
    assert_eq!(out, "4\n");
}

// Was a compile error ("`f` is a variable, not a function") until CRUSH-251
// step 2 gave the VMs function values.
#[test]
fn calling_a_function_value_through_a_parameter() {
    let out = run("fn apply(f, v) { return f(v) }\nfn main() { print(apply(|x| => x * 2, 21)) }");
    assert_eq!(out, "42\n");
}
