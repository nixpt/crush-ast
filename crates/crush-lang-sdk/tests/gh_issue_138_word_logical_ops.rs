//! #138: `and` / `or` / `not` are the word forms of `&&` / `||` / `!` (as the
//! guide documents) — same precedence and the same short-circuit behaviour.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

/// The issue's repro: was `[E-PP02] expected `{`, found `and``.
#[test]
fn and_or_not_keywords() {
    let out = run(r#"
fn main() {
  if true and false { print("yes") } else { print("no") }
  if false or true { print("or") }
  if not false { print("not") }
  print(not (1 > 2) and 3 > 2)
}
"#);
    assert_eq!(out, "no\nor\nnot\ntrue\n");
}

/// Same short-circuit semantics as the symbol forms (#65).
#[test]
fn word_forms_short_circuit() {
    let out = run(r#"
fn t(tag) { print("t" + tag) return true }
fn f(tag) { print("f" + tag) return false }
fn main() {
  print(f("1") and t("2"))
  print(t("3") or f("4"))
}
"#);
    assert_eq!(out, "f1\nfalse\nt3\ntrue\n");
}

/// Word and symbol forms mix and keep `&&`-over-`||` precedence.
#[test]
fn precedence_matches_symbol_forms() {
    let out = run(r#"
fn main() {
  print(true or false and false)
  print(true || false && false)
  print((true or false) and false)
}
"#);
    assert_eq!(out, "true\ntrue\nfalse\n");
}
