#![cfg(feature = "polyglot-rust")]
use crush_lang_sdk::compile::compile_rust_source;

#[test]
fn block_and_loop_locals_do_not_leak() {
    for source in [
        "fn main() { { let x = 42; } println!(\"{}\", x); }",
        "fn main() { for i in 0..1 {} println!(\"{}\", i); }",
        "fn main() { if true { let f = || 42; } println!(\"{}\", f()); }",
    ] {
        let error =
            compile_rust_source(source).expect_err("out-of-scope Rust name must not resolve");
        assert!(format!("{error:#}").contains("Undefined"), "{error:#}");
    }
}
