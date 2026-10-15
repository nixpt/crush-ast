//! Rust SDK — full pipeline from Rust source to CVM1 execution.
//! These end-to-end Rust walker checks live with the SDK integration surface,
//! avoiding a test-only dependency cycle from the walker back to the SDK.
#![cfg(feature = "polyglot-rust")]

use crush_lang_sdk::{Quotas, Runtime, compile::compile_source};

/// Rust source → CVM1 execution → output string.
pub fn run_rust(source: &str) -> anyhow::Result<String> {
    let vm_prog = compile_source("rust", source)?;
    let quotas = Quotas {
        max_steps: 10_000_000,
        ..Default::default()
    };
    let result = Runtime::with_quotas(quotas).run(&vm_prog)?;
    Ok(result.output.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        assert_eq!(
            run_rust("fn main() { let x = 42; println(x + 1); }").unwrap(),
            "43"
        );
    }

    #[test]
    fn test_function() {
        assert_eq!(
            run_rust(
                "fn double(n: i32) -> i32 { return n + n; } fn main() { println(double(21)); }"
            )
            .unwrap(),
            "42"
        );
    }

    #[test]
    fn test_if_else() {
        let src =
            "fn main() { let x = 5; if x > 3 { println(\"yes\"); } else { println(\"no\"); } }";
        assert_eq!(run_rust(src).unwrap(), "yes");
    }

    #[test]
    fn test_while_loop() {
        let src = "fn main() { let mut i = 0; while i < 5 { i = i + 1; } println(i); }";
        assert_eq!(run_rust(src).unwrap(), "5");
    }

    #[test]
    fn test_for_loop() {
        let src = "fn main() { let mut sum = 0; for i in 0..10 { sum = sum + i; } println(sum); }";
        assert_eq!(run_rust(src).unwrap(), "45");
    }

    #[test]
    fn walker_assignment_preserves_loop_carried_values() {
        assert_eq!(
            run_rust("fn main() { let mut sum = 1; for i in 0..4 { sum += i; } println(sum); }")
                .unwrap(),
            "7"
        );
        assert_eq!(run_rust("fn main() { let mut n = 0; while n < 3 { if n == 1 { n += 1; } else { n = n + 1; } } println(n); }").unwrap(), "3");
    }

    #[test]
    #[cfg(feature = "polyglot-javascript")]
    fn javascript_assignment_uses_the_same_compiler_contract() {
        let p = compile_source("js", "var n = 0; while (n < 3) { n++; } console.log(n);").unwrap();
        assert_eq!(Runtime::new().run(&p).unwrap().output.trim(), "3");
    }

    #[test]
    #[cfg(feature = "polyglot-python")]
    fn python_invalid_assignment_contract_is_rejected() {
        // The Python named-expression walker currently reverses the helper's
        // operands. The shared path must reject it before VM execution.
        let error = compile_source("py", "(x := 41)\nprint(x)\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("__crush_assign__ target must be a variable")
        );
    }

    #[test]
    fn test_multi_statement_function() {
        let src = "fn add(a: i32, b: i32) -> i32 { return a + b; } fn main() { let x = add(10, 20); println(x); }";
        assert_eq!(run_rust(src).unwrap(), "30");
    }

    #[test]
    fn test_array_literal() {
        let src = "fn main() { let arr = [1, 2, 3]; println(arr[0]); }";
        assert_eq!(run_rust(src).unwrap(), "1");
    }

    #[test]
    fn test_closure_basic() {
        let src = "fn main() { let add = |x: i32| x + 1; println(add(41)); }";
        assert_eq!(run_rust(src).unwrap(), "42");
    }

    #[test]
    fn closure_parameters_shadow_outer_bindings_and_keep_argument_order() {
        let src = "fn main() { let x = 100; let sub = |x: i32, y: i32| x - y; println(sub(50, 8)); println(x); }";
        assert_eq!(run_rust(src).unwrap(), "42\n100");
    }

    #[test]
    fn closure_reads_a_scalar_capture() {
        assert_eq!(
            run_rust(
                "fn main() { let offset = 40; let add = |x: i32| offset + x; println(add(2)); }"
            )
            .unwrap(),
            "42"
        );
    }

    #[test]
    fn unsupported_closure_forms_fail_before_execution() {
        for (src, expected) in [
            (
                "fn main() { let add = move |x: i32| x + 1; }",
                "unsupported Rust move closure",
            ),
            (
                "fn main() { let add = async |x: i32| x + 1; }",
                "unsupported Rust async closure",
            ),
            (
                "fn main() { let add = |(x, y)| x + y; }",
                "unsupported Rust closure parameter",
            ),
            (
                "fn main() { let add = |ref x| x; }",
                "unsupported Rust closure parameter",
            ),
            (
                "fn main() { let add = |x: i32| x + missing; }",
                "Undefined variable: missing",
            ),
            (
                "fn main() { let add = |x: i32| x + 1; add(); }",
                "expects 1 arguments",
            ),
        ] {
            let error = compile_source("rust", src).unwrap_err();
            assert!(error.to_string().contains(expected), "{src}: {error:#}");
        }
    }

    #[test]
    fn expression_blocks_are_exact_or_rejected_at_their_source_location() {
        assert_eq!(
            run_rust(
                "fn main() { let x = { 40 + 2 }; let y = if true { x } else { 0 }; println(y); }"
            )
            .unwrap(),
            "42"
        );
        for src in [
            "fn main() {\n    let x = { 1; 42 };\n}",
            "fn main() {\n    let x = { let y = 1; y + 41 };\n}",
            "fn main() {\n    let x = { 42; };\n}",
            "fn main() {\n    let x = if true { println!(\"effect\"); 42 } else { 0 };\n}",
        ] {
            let error = compile_source("rust", src).unwrap_err().to_string();
            assert!(
                error.contains("unsupported Rust block expression at 2:"),
                "{src}: {error}"
            );
            assert!(!error.contains("Undefined function"), "{error}");
        }
    }

    #[test]
    fn returns_in_value_positions_fail_during_lowering() {
        let src = "fn main() {\n    let x = (return);\n}";
        let error = compile_source("rust", src).unwrap_err().to_string();
        assert!(
            error.contains("unsupported Rust return in a value expression at 2:"),
            "{error}"
        );
    }
}
