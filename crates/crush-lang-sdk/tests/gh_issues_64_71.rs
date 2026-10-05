//! Regressions for the GitHub issues fixed together in CRUSH-124/127/128/129
//! (#64, #67, #68, #69), run through the real pipeline: Crush source →
//! parse → compile → CVM1. Each program is the issue's own repro or a
//! direct extension of it. (#71 / CRUSH-131 is covered at the CAST level in
//! crush-frontend's optimizer_tests.rs — it needs a `@lang` block.)

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> Result<String, String> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .map_err(|e| format!("compile: {e}"))?;
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

/// #64 / CRUSH-124: `"\r"` is a carriage return, not the letter `r`.
#[test]
fn carriage_return_escape_survives_compilation() {
    let out = run(r#"
fn main() {
  let r = "\r"
  if r == "r" { print("BUG: \\r is letter r") } else { print("ok") }
  print(len("a\r\nb"))
}
"#)
    .unwrap();
    assert_eq!(out, "ok\n4\n");
}

/// #67 / CRUSH-127: an uncaught throw keeps its message and is not
/// reported as a missing capability.
#[test]
fn uncaught_throw_is_not_an_unknown_capability() {
    let err = run(r#"fn main() { throw "line 5, col 5: expected a value" }"#).unwrap_err();
    assert_eq!(err, "uncaught error: line 5, col 5: expected a value");
}

/// #68 / CRUSH-128: prefix `!` and `-` apply to the whole postfix chain.
#[test]
fn prefix_operators_apply_to_calls_indexing_and_fields() {
    let out = run(r#"
fn is_digit(c) { return c == "5" }
fn three() { return 3 }
fn main() {
  if !is_digit("x") { print("not a digit") }
  print(-three())
  let a = [1, 2]
  print(-a[1])
}
"#)
    .unwrap();
    assert_eq!(out, "not a digit\n-3\n-2\n");
}

/// #68 / CRUSH-128: …without capturing binary operators.
#[test]
fn prefix_operators_still_bind_tighter_than_binary_operators() {
    let out = run(r#"
fn main() {
  print(-2 * 3)
  print(- 2 + 3)
  print(!false && false)
  print(!true || true)
}
"#)
    .unwrap();
    assert_eq!(out, "-6\n1\nfalse\ntrue\n");
}

/// #69 / CRUSH-129: a top-level `main()` next to `fn main` runs main once.
#[test]
fn trailing_main_call_runs_main_once() {
    let out = run("fn main() { print(\"hi\") }\nmain()\n").unwrap();
    assert_eq!(out, "hi\n");
}

/// #69 / CRUSH-129: only the bare call is dropped — other top-level
/// statements still run before main's body.
#[test]
fn other_top_level_statements_still_run_before_main() {
    let out = run("print(\"init\")\nfn main() { print(\"hi\") }\nmain()\n").unwrap();
    assert_eq!(out, "init\nhi\n");
}
