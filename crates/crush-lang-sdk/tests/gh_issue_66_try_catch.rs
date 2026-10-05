//! #66 / CRUSH-126: a throw caught across a call boundary must unwind to the
//! frame that entered the `try`. The VMs used to jump to the handler while
//! still inside the callee, so the catch block ran in the wrong frame and the
//! code after the try/catch ran a second time when the callee returned.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

/// The issue's repro 1: the continuation ran twice (`A caught C C`).
#[test]
fn code_after_a_caught_throw_runs_once() {
    let out = run(r#"
fn boom() { throw "kaboom" return 0 }
fn main() {
  print("A")
  try { boom() } catch e { print("caught") }
  print("C")
}
"#);
    assert_eq!(out, "A\ncaught\nC\n");
}

/// The issue's repro 2: the rest of the try block re-ran with null locals.
#[test]
fn rest_of_the_try_block_does_not_rerun() {
    let out = run(r#"
fn thrower() { throw "x" return 0 }
fn main() {
  try {
    let v = thrower()
    print("v: " + v)
  } catch e { print("caught") }
  print("done")
}
"#);
    assert_eq!(out, "caught\ndone\n");
}

/// Throws several frames deep, `return` inside catch, and a handler left
/// behind by `return` inside a try (it must not catch a later throw).
#[test]
fn unwinding_across_frames_and_stale_handlers() {
    let out = run(r#"
fn inner() { throw "deep" return 0 }
fn middle() { let r = inner() print("NOT REACHED middle") return r }
fn early() { try { return 7 } catch e { print("NOT REACHED early") } return 0 }
fn handled() {
  try { return middle() } catch e { print("handled caught " + e) return -1 }
  return 0
}
fn main() {
  print(handled())
  print(early())
  try { throw "after-early" } catch e { print("main caught " + e) }
  try { let x = inner() } catch e { print("again " + e) }
  print("end")
}
"#);
    assert_eq!(
        out,
        "handled caught deep\n-1\n7\nmain caught after-early\nagain deep\nend\n"
    );
}
