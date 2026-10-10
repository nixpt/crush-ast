//! GitHub issues #143 and #147: match arms were second-class in two places.
//!
//! - #143: a void capability call (`print`) as an arm's last expression left
//!   nothing on the stack, but the arm's `JMP` lands on the shared `POP` after
//!   the match, so the program died with `stack underflow`. The CASM
//!   post-pass only dropped a `POP` directly after the call.
//! - #147: the optimizer's mutation scan never looked inside match arms (or
//!   lambda bodies), so `let r = "a"; match 2 { 2 => { r = "b" } }; print(r)`
//!   was constant-folded to `print("a")` — `crush run` (which always
//!   optimizes) disagreed with an unoptimized `crushc`.

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
fn void_call_ending_a_match_arm_does_not_underflow() {
    let out = run(r#"
fn main() {
  let x = 1
  match x {
    1 => { print("one") }
    _ => { print("other") }
  }
  match 7 {
    1 => print("no")
    _ => print("fallback")
  }
  print("after")
}
"#);
    assert_eq!(out, "one\nfallback\nafter\n");
}

#[test]
fn match_value_with_void_arm_is_null() {
    let out = run(r#"
fn main() {
  let v = match 2 {
    1 => 10
    _ => { print("side") }
  }
  print(v)
}
"#);
    assert_eq!(out, "side\nnull\n");
}

#[test]
fn void_call_value_can_be_stored_and_returned() {
    let out = run(r#"
fn f() { return print("in f") }
fn main() {
  let a = print("x")
  print(a)
  print(f())
}
"#);
    assert_eq!(out, "x\nnull\nin f\nnull\n");
}

#[test]
fn assignment_inside_a_match_arm_survives_the_match() {
    let out = run(r#"
fn main() {
  let r = "a"
  match 2 {
    2 => { r = "b" }
    _ => { r = "c" }
  }
  print(r)
  let n = 0
  match 1 {
    1 => { n = n + 5 }
    _ => { n = 0 }
  }
  print(n)
  let k = 1
  if true {
    match 3 {
      3 => { k = 30 }
      _ => { k = 0 }
    }
  }
  print(k)
}
"#);
    assert_eq!(out, "b\n5\n30\n");
}

#[test]
fn assignment_inside_a_match_arm_inside_a_loop_is_seen() {
    let out = run(r#"
fn main() {
  let total = 0
  let i = 0
  while i < 3 {
    match i {
      1 => { total = total + 10 }
      _ => { total = total + 1 }
    }
    i = i + 1
  }
  print(total)
}
"#);
    assert_eq!(out, "12\n");
}
