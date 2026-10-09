//! CRUSH-187 (GitHub #37, #92 pong) and CRUSH-189 (GitHub #92 game_of_life):
//! end-to-end regressions through `compile_crush_source`, which runs the
//! optimizer the same way `crush-run x.crush` does.

use crush_lang_sdk::compile::compile_crush_source;
use crush_lang_sdk::{Quotas, Runtime};

fn run(source: &str) -> String {
    let prog = compile_crush_source(source).expect("compile");
    let result = Runtime::new().run(&prog).expect("run");
    assert!(result.halted);
    result.output
}

// ── CRUSH-187: implicit return after an early return ──────────────────────

#[test]
fn early_return_then_fall_off_the_end() {
    let out = run(r#"
fn f(c) {
  if c { return }
  print("after")
}
f(false)
print("ok")
"#);
    assert_eq!(out.lines().collect::<Vec<_>>(), ["after", "ok"]);
}

#[test]
fn early_return_value_then_fall_off_the_end_returns_null() {
    let out = run(r#"
fn early(x) {
  if x { return 1 }
}
print(early(true))
print(early(false))
"#);
    assert_eq!(out.lines().collect::<Vec<_>>(), ["1", "null"]);
}

#[test]
fn github_37_if_return_else_call() {
    // GitHub #37's repro, verbatim.
    let out = run(r#"
fn count(n, max) {
    io.print(n);
    if n >= max {
        return;
    } else {
        count(n + 1, max);
    }
}
fn main() {
    count(0, 5);
}
"#);
    assert_eq!(out.lines().collect::<Vec<_>>(), ["0", "1", "2", "3", "4", "5"]);
}

#[test]
fn recursive_tail_call_without_trailing_return() {
    // Shape of awesome-crush pong's `pong_tick` (GitHub #92).
    let out = run(r#"
fn tick(n) {
  if n == 0 {
    print("end")
    return
  }
  tick(n - 1)
}
tick(5)
print("ok")
"#);
    assert_eq!(out.lines().collect::<Vec<_>>(), ["end", "ok"]);
}

// ── CRUSH-189: optimizer soundness ────────────────────────────────────────

#[test]
fn mul_by_two_evaluates_operand_once() {
    let out = run(r#"
fn f() {
  print("called")
  return 5
}
print(2 * f())
print(f() * 2)
"#);
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        ["called", "10", "called", "10"]
    );
}

#[test]
fn pow2_recursion_stays_linear() {
    // game_of_life's `2 * pow2(n - 1)`: exponential once rewritten to `e + e`.
    let prog = compile_crush_source(
        r#"
fn pow2(n) {
  if n == 0 { return 1 }
  return 2 * pow2(n - 1)
}
print(pow2(30))
"#,
    )
    .expect("compile");
    let result = Runtime::with_quotas(Quotas { max_steps: 100_000, ..Quotas::default() }).run(&prog).expect("run");
    assert_eq!(result.output.trim(), "1073741824");
}

#[test]
fn zero_times_call_still_calls() {
    let out = run(r#"
fn f() {
  print("called")
  return 5
}
print(0 * f())
"#);
    assert_eq!(out.lines().collect::<Vec<_>>(), ["called", "0"]);
}

#[test]
fn string_plus_zero_is_concatenation() {
    let out = run(r#"
let s = "a"
print(s + 0)
print(0 + s)
"#);
    assert_eq!(out.lines().collect::<Vec<_>>(), ["a0", "0a"]);
}

#[test]
fn constant_fold_overflow_is_a_runtime_error_not_a_panic() {
    let prog = compile_crush_source(
        r#"
let a = 9223372036854775807
print(a + 1)
"#,
    )
    .expect("compile must not panic");
    let err = Runtime::new().run(&prog).expect_err("overflow");
    assert!(err.to_string().contains("overflow"), "{err}");
}

#[test]
fn catch_sees_assignments_made_in_try() {
    let out = run(r#"
let t = 1
try {
  t = 2
  throw "x"
} catch e {
  print(t)
}
"#);
    assert_eq!(out.trim(), "2");
}
