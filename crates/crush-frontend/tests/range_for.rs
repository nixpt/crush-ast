//! Range expressions (`a..b`) and `for i in a..b`.
//!
//! Ranges were lexed (`Token::DotDot`) and had an AST node (`Expression::Range`) and a
//! renderer arm — but NO parser, in crush-ast OR in exosphere's in-tree crush-lang, ever
//! constructed one. `for i in 0..3` had therefore never parsed anywhere.
//! exosphere's `tests/language/for_loop_test.crush` asserts it works and was never wired
//! into a test runner, so nothing caught it.

use crush_frontend::compile_crush_source;

fn compiles(src: &str) -> bool {
    compile_crush_source(src).is_ok()
}

#[test]
fn range_for_loop_parses() {
    assert!(compiles("fn main() { for i in 0..3 { print(i); } }"));
}

#[test]
fn range_bounds_may_be_expressions() {
    // Precedence check: `..` must bind LOOSER than `+`, so this is `0..(n+1)`, not `(0..n)+1`.
    assert!(compiles("fn main() { let n = 2; for i in 0..n+1 { print(i); } }"));
}

#[test]
fn empty_range_is_legal() {
    assert!(compiles("fn main() { for i in 0..0 { print(i); } }"));
}

#[test]
fn break_and_continue_work_inside_a_range_loop() {
    assert!(compiles("fn main() { for i in 0..9 { if i > 1 { break; } print(i); } }"));
}

#[test]
fn nested_ranges_do_not_collide() {
    // Each loop allocates its own __end_N temp; a shared one would break the outer loop.
    assert!(compiles("fn main() { for i in 0..2 { for j in 0..2 { print(j); } } }"));
}

#[test]
fn array_for_loops_still_work() {
    // Regression: the range path is a new early-return in compile_stmt. The array path
    // below it must be untouched.
    assert!(compiles("fn main() { for x in [1,2,3] { print(x); } }"));
}

// ── `async` as a contextual keyword ──────────────────────────────────────────────────────
//
// The lexer emits Token::Async for `async` unconditionally, which made the `async.*`
// CAPABILITY NAMESPACE unreachable. exosphere's async_test.crush calls `await async.sleep(100)`
// — `async` there is a namespace, not a keyword — and it died at parse with
// "Unexpected token in expression: Async". `await` itself was never broken.

#[test]
fn async_namespace_is_reachable() {
    assert!(compiles("fn main() { await async.sleep(1); }"));
}

#[test]
fn async_namespace_without_await() {
    assert!(compiles("fn main() { async.sleep(1); }"));
}

#[test]
fn await_on_a_normal_call_still_parses() {
    // Regression: `await` was fine before this change and must stay fine.
    assert!(compiles("fn main() { await foo.bar(1); }"));
}

// ── field assignment + yield ─────────────────────────────────────────────────────────────
//
// Same shape as ranges, twice more: the CAST node existed AND the compiler already lowered it,
// and only the parser never built one.
//   `p.x = 10`  -> Statement::SetField   (died: "Unexpected token in expression: Assign")
//   `yield;`    -> Expression::Yield     (died: "Unexpected token in expression: Yield")

#[test]
fn field_assignment_parses() {
    assert!(compiles("struct P { x } fn main() { let p = new P(); p.x = 10; }"));
}

#[test]
fn nested_field_assignment_parses() {
    assert!(compiles(
        "struct I { v } struct O { i } fn main() { let o = new O(); o.i.v = 5; }"
    ));
}

#[test]
fn plain_assignment_still_parses() {
    // Regression: the SetField arm restructured the Assign branch.
    assert!(compiles("fn main() { let x = 0; x = 1; }"));
}

#[test]
fn bare_yield_parses() {
    assert!(compiles("fn main() { print(\"a\"); yield; print(\"b\"); }"));
}
