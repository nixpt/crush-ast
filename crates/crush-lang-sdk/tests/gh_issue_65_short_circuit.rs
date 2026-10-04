//! #65 / CRUSH-125: `&&` / `||` evaluate their right operand only when the
//! left one doesn't decide the result. They used to compile to eager
//! `and`/`or` opcodes, so the bounds-check idiom indexed out of range.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

/// The issue's repro: was `array index out of range: 3 (len 3)`.
#[test]
fn bounds_check_idiom_does_not_index_out_of_range() {
    let out = run(r#"
fn main() {
  let s = "abc"
  let i = 3
  if i < len(s) && s[i] == "x" { print("true") } else { print("false") }
}
"#);
    assert_eq!(out, "false\n");
}

/// The trace shows exactly which operands ran, and in what order.
#[test]
fn right_operand_runs_only_when_needed() {
    let out = run(r#"
fn t(tag) { print("t" + tag) return true }
fn f(tag) { print("f" + tag) return false }
fn main() {
  print(f("1") && t("2"))
  print(t("3") || f("4"))
  print(t("5") && f("6"))
  print(f("7") || t("8"))
  let n = 10
  print(n > 5 && n < 20 || f("9"))
  if !(f("a") && t("b")) { print("not") }
}
"#);
    assert_eq!(
        out,
        "f1\nfalse\nt3\ntrue\nt5\nf6\nfalse\nf7\nt8\ntrue\ntrue\nfa\nnot\n"
    );
}
