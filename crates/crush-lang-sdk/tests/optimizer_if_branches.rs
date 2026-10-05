//! CRUSH-143: the optimizer restored the constants it knew before an `if`
//! and forgot that either branch might have reassigned them, so
//! `let n = 0; if c { n = n + 1 }; print(n)` printed 0. Found while writing
//! CRUSH-136's tests.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

#[test]
fn assignments_inside_if_branches_survive_the_if() {
    let out = run(r#"
fn yes() { return true }
fn no() { return false }
fn main() {
  let n = 0
  if yes() { n = n + 1 }
  print(n)
  let m = 0
  if no() { m = 5 } else { m = 7 }
  print(m)
  let k = 1
  if yes() { if yes() { k = k * 10 } }
  print(k)
  let u = 3
  if no() { u = 99 }
  print(u)
}
"#);
    assert_eq!(out, "1\n7\n10\n3\n");
}

#[test]
fn a_folded_condition_keeps_its_branch_constants() {
    let out = run("fn main() {\n  let n = 1\n  if true { n = 2 }\n  print(n)\n}\n");
    assert_eq!(out, "2\n");
}
