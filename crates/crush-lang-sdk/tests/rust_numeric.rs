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
fn cast_of_effectful_operand_is_rejected_before_execution() {
    let source = "fn tick() -> i64 { println!(\"called\"); 300 }\nfn main() { println!(\"{}\", tick() as u8); }";
    let error = format!("{:#}", compile_rust_source(source).unwrap_err());
    assert!(error.contains("nonliteral Rust cast operand"), "{error}");
    assert!(error.contains("at 2:"), "{error}");
    assert!(error.contains("CRUSH-xps-8"), "{error}");
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
