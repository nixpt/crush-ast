//! #74 / #77 / CRUSH-134: field access on an `any` value is a dynamic map
//! lookup (missing key → null), and an `any` condition is tested for
//! truthiness. A condition statically known not to be bool is still a
//! compile error.

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

/// #74 repro 1: was `Cannot access field 'pos' on non-struct type any`.
#[test]
fn field_access_on_param() {
    let src = "fn getpos(p) { return p.pos }\nfn main() {\n  let m = {\"pos\": 42}\n  print(getpos(m))\n}\n";
    assert_eq!(run(src).unwrap(), "42\n");
}

/// #74 repro 2: was `If condition must be bool, found any`.
#[test]
fn map_field_as_condition() {
    let src = "fn main() {\n  let m = {\"flag\": true}\n  if m.flag { print(\"on\") }\n  let off = {\"flag\": false}\n  if off.flag { print(\"wrong\") } else { print(\"off\") }\n}\n";
    assert_eq!(run(src).unwrap(), "on\noff\n");
}

/// #77 repro: chained access, directly and through an intermediate local.
#[test]
fn chained_field_access() {
    let src = r#"
fn main() {
  let m = {"outer": {"inner": 42}}
  let o = m.outer
  print(o.inner)
  print(m.outer.inner)
}
"#;
    assert_eq!(run(src).unwrap(), "42\n42\n");
}

#[test]
fn missing_key_is_null() {
    let src = "fn get(p) { return p.nope }\nfn main() {\n  print(get({\"a\": 1}))\n  let m = {\"a\": 1}\n  if m.nope { print(\"wrong\") } else { print(\"falsy\") }\n}\n";
    assert_eq!(run(src).unwrap(), "null\nfalsy\n");
}

#[test]
fn any_condition_in_while() {
    let src = "fn main() {\n  let m = {\"n\": 3}\n  let s = {\"go\": true}\n  let i = 0\n  while s.go {\n    i = i + 1\n    if i == m.n { s.go = false }\n  }\n  print(i)\n}\n";
    assert_eq!(run(src).unwrap(), "3\n");
}

/// The decision keeps a condition known not to be bool an error.
#[test]
fn non_bool_literal_condition_still_rejected() {
    let err = run("fn main() {\n  if 5 { print(\"x\") }\n}\n").unwrap_err();
    assert!(err.contains("condition must be bool"), "{err}");
    let err = run("fn main() {\n  while \"s\" { print(\"x\") }\n}\n").unwrap_err();
    assert!(err.contains("condition must be bool"), "{err}");
}
