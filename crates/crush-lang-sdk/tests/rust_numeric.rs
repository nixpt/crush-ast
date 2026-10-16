//! xps-8: exercise the numeric boundary through the public SDK.
#![cfg(all(feature = "polyglot-rust", feature = "stdlib"))]

use crush_lang_sdk::{HostCapsBuilder, Runtime, compile::compile_rust_source};

#[test]
fn literal_cast_result_can_participate_in_integer_arithmetic() {
    let program = compile_rust_source(
        "fn main() { let a = 255u8 as i8 as i64; let b = 300i64 as u8; println!(\"{} {}\", a / 2, b / 3); }",
    ).unwrap();
    let result = Runtime::new()
        .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
        .run(&program)
        .unwrap();
    // Integer truncating division distinguishes real Int values from a
    // float result which merely prints like an integer before further use.
    assert_eq!(result.output, "0 14\n");
}

#[test]
fn cast_of_effectful_operand_runs_once() {
    let source = "fn tick() -> i64 { println!(\"called\"); 300 }\nfn main() { println!(\"{}\", tick() as u8); }";
    let program = compile_rust_source(source).unwrap();
    let result = Runtime::new()
        .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
        .run(&program)
        .unwrap();
    assert_eq!(result.output, "called\n44\n");
}

#[test]
fn unproven_returns_and_call_arguments_cannot_enter_a_cast() {
    for source in [
        "fn bump(n: u8) -> u8 { n + 10 } fn main() { println!(\"{}\", bump(250) as u16); }",
        "fn cast(n: u8) -> i64 { n as i64 } fn bump(n: u8) -> u8 { n + 10 } fn main() { println!(\"{}\", cast(bump(250))); }",
        "fn bad(n: i64) -> i64 { n + 1 } fn main() { println!(\"{}\", bad(9223372036854775807) as u8); }",
        "fn __crush_rust_value__(n: u8) -> u8 { n + 10 } fn main() { println!(\"{}\", __crush_rust_value__(250) as u16); }",
        "fn main() { let mut n = 300i64; n = 400; println!(\"{}\", n as u8); }",
    ] {
        let error = format!("{:#}", compile_rust_source(source).unwrap_err());
        let (boundary, ticket) = ("unsupported Rust numeric operation at", "CRUSH-xps-8");
        assert!(
            error.contains(boundary) && error.contains(ticket),
            "{error}"
        );
    }
}

#[test]
fn crush_arithmetic_keeps_its_existing_policy() {
    let program =
        crush_lang_sdk::compile::compile_source("crush", "fn main() { return 300 + 10; }").unwrap();
    assert_eq!(
        Runtime::new().run(&program).unwrap().return_value,
        Some(crush_lang_sdk::Value::Int(310))
    );
}

#[test]
fn bounded_unsigned_operations_refuse_unrepresentable_results() {
    for source in [
        "fn high(n: u64) -> u64 { n + 1 } fn main() { println!(\"{}\", high(9223372036854775807)); }",
        "fn low(n: u64) -> u64 { n - 1 } fn main() { println!(\"{}\", low(0)); }",
        "fn main() { let n = 5u64; println!(\"{}\", !n); }",
        "fn main() { println!(\"{}\", 2u64.pow(63)); }",
        "fn main() { let n: usize = 70000; println!(\"{}\", n as i64); }",
        "fn main() { let n = 65535usize; println!(\"{}\", n + 1); }",
    ] {
        let error = format!("{:#}", compile_rust_source(source).unwrap_err());
        assert!(
            error.contains(if source.contains(".pow(") {
                "CRUSH-xps-13"
            } else {
                "CRUSH-xps-8"
            }) && error.contains(" at 1:"),
            "{error}"
        );
    }
}
