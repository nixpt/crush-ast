//! #142: single-quoted strings are strings; `b"..."` (no bytes type yet) gets
//! a clear error instead of "Undefined variable: b".

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

/// The issue's repro: was `[E-PP01] Unexpected character: '`.
#[test]
fn single_quoted_strings() {
    let out = run(r#"
fn main() {
  let s = 'hello'
  print(s)
  print('it\'s "quoted"' + ' and ' + "it's")
  print('tab\tend' == "tab\tend")
  print(len('abc'))
}
"#);
    assert_eq!(out, "hello\nit's \"quoted\" and it's\ntrue\n3\n");
}

#[test]
fn bytes_literal_is_a_clear_error() {
    let err = crush_lang_sdk::compile::compile_crush_source("fn main() { let d = b\"data\" }")
        .expect_err("bytes literals are not supported yet");
    let msg = format!("{err:#}");
    assert!(msg.contains("bytes literals"), "{msg}");
    assert!(!msg.contains("Undefined variable"), "{msg}");
}

/// An identifier named `b` still works when it isn't glued to a string.
#[test]
fn identifier_b_unaffected() {
    let out = run(r#"
fn main() {
  let b = "x"
  print(b + "y")
}
"#);
    assert_eq!(out, "xy\n");
}
