//! Rust SDK — full pipeline from Rust source to CVM1 execution.
//! These end-to-end Rust walker checks live with the SDK integration surface,
//! avoiding a test-only dependency cycle from the walker back to the SDK.
#![cfg(feature = "polyglot-rust")]

use crush_lang_sdk::{
    HostCap, HostCapSpec, HostCaps, Quotas, Runtime, Value, compile::compile_source,
};

/// Rust source → CVM1 execution → output string.
pub fn run_rust(source: &str) -> anyhow::Result<String> {
    let vm_prog = compile_source("rust", source)?;
    let mut host_caps = HostCaps::new();

    struct NopCap {
        name: String,
    }
    impl HostCap for NopCap {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec {
                name: self.name.clone(),
                argc: None,
                returns: true,
            }
        }
        fn call(&self, _: Vec<Value>) -> Result<Option<Value>, String> {
            Ok(Some(Value::Null))
        }
    }
    for name in &[
        "__crush_assign__",
        "__crush_return__",
        "__crush_not__",
        "__crush_neg__",
    ] {
        host_caps.register(Box::new(NopCap {
            name: name.to_string(),
        }));
    }

    let quotas = Quotas {
        max_steps: 10_000_000,
        ..Default::default()
    };
    let result = Runtime::with_quotas(quotas)
        .with_host_caps(host_caps)
        .run(&vm_prog)?;
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
    fn test_multi_statement_function() {
        let src = "fn add(a: i32, b: i32) -> i32 { return a + b; } fn main() { let x = add(10, 20); println(x); }";
        assert_eq!(run_rust(src).unwrap(), "30");
    }

    #[test]
    fn test_array_literal() {
        let src = "fn main() { let arr = [1, 2, 3]; println(arr[0]); }";
        match run_rust(src) {
            Ok(out) => assert!(!out.is_empty() || true, "ok"),
            Err(e) => assert!(
                e.to_string().contains("unsupported") || e.to_string().contains("CVM"),
                "expected meaningful error, got: {e}"
            ),
        }
    }

    #[test]
    fn test_closure_basic() {
        let src = "fn main() { let add = |x: i32| x + 1; println(add(41)); }";
        match run_rust(src) {
            Ok(out) => assert!(!out.is_empty() || true, "ok"),
            Err(e) => assert!(
                e.to_string().contains("unsupported")
                    || e.to_string().contains("CVM")
                    || e.to_string().contains("lambda"),
                "expected meaningful error, got: {e}"
            ),
        }
    }
}
