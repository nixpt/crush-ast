//! #76 / CRUSH-136: `<` / `>` / `<=` / `>=` on two strings compare
//! lexicographically by code point (= UTF-8 byte order) on every backend;
//! a string against a number is still a runtime type error.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> Result<String, String> {
    let program =
        crush_lang_sdk::compile::compile_crush_source(source).map_err(|e| e.to_string())?;
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

/// The issue's repro: was `type error: expected numeric, got str`.
#[test]
fn issue_repro_prints_lt() {
    assert_eq!(
        run("fn main() {\n  if \"a\" < \"b\" { print(\"lt\") }\n}\n").unwrap(),
        "lt\n"
    );
}

#[test]
fn ordering_is_by_code_point() {
    let out = run(r#"
fn main() {
  print("apple" < "b")
  print("B" < "a")
  print("app" <= "apple")
  print("b" > "a")
  print("x" >= "x")
  print("" < "a")
  print("é" > "z")
}
"#)
    .unwrap();
    assert_eq!(out, "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
}

#[test]
fn string_against_number_is_still_a_type_error() {
    let err = run("fn lt(a: any, b: any) { return a < b }\nfn main() { print(lt(\"a\", 1)) }\n")
        .unwrap_err();
    assert!(err.contains("type error"), "{err}");
}
