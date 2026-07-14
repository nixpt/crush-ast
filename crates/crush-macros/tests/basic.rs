//! Integration tests for crush! and crush_file! macros.

use crush_macros::{crush, crush_file};
use crush_vm::CrushResultExt;

// ── crush! string literal form ─────────────────────────────────────────────

#[test]
fn test_crush_string_literal_int() {
    let result = crush!("fn main() { return 42; }");
    assert_eq!(result.crush_unwrap_int(), 42);
}

#[test]
fn test_crush_string_literal_float() {
    let result = crush!("fn main() { return 3.14; }");
    let val = result.crush_unwrap_float();
    assert!((val - 3.14).abs() < 0.001);
}

#[test]
fn test_crush_string_literal_bool_true() {
    let result = crush!("fn main() { return true; }");
    assert_eq!(result.crush_unwrap_bool(), true);
}

#[test]
fn test_crush_string_literal_bool_false() {
    let result = crush!("fn main() { return false; }");
    assert_eq!(result.crush_unwrap_bool(), false);
}

#[test]
fn test_crush_string_literal_null() {
    let result = crush!("fn main() { return null; }");
    assert!(result.crush_is_null());
}

#[test]
fn test_crush_string_literal_arithmetic() {
    let result = crush!("fn main() { let x = 100; let y = 23; return x - y; }");
    assert_eq!(result.crush_unwrap_int(), 77);
}

#[test]
fn test_crush_string_literal_mul() {
    let result = crush!("fn main() { return 7 * 6; }");
    assert_eq!(result.crush_unwrap_int(), 42);
}

#[test]
fn test_crush_string_literal_div() {
    let result = crush!("fn main() { return 100 / 4; }");
    assert_eq!(result.crush_unwrap_int(), 25);
}

// ── crush! raw block form ──────────────────────────────────────────────────

#[test]
fn test_crush_raw_block_int() {
    let result = crush!({
        fn main() {
            return 42;
        }
    });
    assert_eq!(result.crush_unwrap_int(), 42);
}

#[test]
fn test_crush_raw_block_complex() {
    let result = crush!({
        fn main() {
            let a = 20;
            let b = 22;
            return a + b;
        }
    });
    assert_eq!(result.crush_unwrap_int(), 42);
}

#[test]
fn test_crush_raw_block_bool() {
    let result = crush!({
        fn main() {
            return true;
        }
    });
    assert_eq!(result.crush_unwrap_bool(), true);
}

// ── crush_file! tests ──────────────────────────────────────────────────────

#[test]
fn test_crush_file_hello() {
    let result = crush_file!("tests/fixtures/hello.crush");
    assert_eq!(result.crush_unwrap_int(), 42);
}

#[test]
fn test_crush_file_add() {
    let result = crush_file!("tests/fixtures/add.crush");
    assert_eq!(result.crush_unwrap_int(), 42);
}

#[test]
fn test_crush_file_float() {
    let result = crush_file!("tests/fixtures/float.crush");
    let val = result.crush_unwrap_float();
    assert!((val - 3.14).abs() < 0.001);
}

#[test]
fn test_crush_file_bool() {
    let result = crush_file!("tests/fixtures/bool.crush");
    assert_eq!(result.crush_unwrap_bool(), true);
}

// ── Smoke tests ────────────────────────────────────────────────────────────

#[test]
fn test_crush_div_expression() {
    // Division at VM level — smoke test that it doesn't panic at compile time
    let result = crush!("fn main() { return 1 / 1; }");
    let _ = result;
}

#[test]
fn test_crush_empty_returns_null() {
    let result = crush!("fn main() { }");
    assert!(result.crush_is_null());
}
