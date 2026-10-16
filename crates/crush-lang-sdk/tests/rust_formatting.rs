//! CRUSH-xps-9: formatting boundaries through the public SDK.
#![cfg(all(feature = "polyglot-rust", feature = "stdlib"))]
use crush_lang_sdk::{HostCapsBuilder, Runtime, compile::compile_rust_source};

#[test]
fn literal_debug_retains_rust_types_and_owned_format_result() {
    let source = include_str!("rustc_corpus/debug_literals.rs");
    let program = compile_rust_source(source).unwrap();
    let result = Runtime::new()
        .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
        .run(&program)
        .unwrap();
    assert_eq!(
        result.output,
        include_str!("rustc_corpus/debug_literals.out")
    );
}

#[test]
fn unproven_debug_and_effectful_placeholder_reuse_fail_before_execution() {
    for source in [
        include_str!("rustc_corpus/debug_dynamic.rs"),
        include_str!("rustc_corpus/format_repeated_effect.rs"),
        include_str!("rustc_corpus/format_reordered_effect.rs"),
        "fn tick() -> i64 { 7 } fn main() { let n = 2; println!(\"{n} {}\", tick()); }",
        "fn main() { println!(\"{x}\", x = 2); }",
        "fn main() { println!(\"{:?}\", 4294967295); }",
        "fn main() { println!(\"{:?}\", 1e999f64); }",
    ] {
        let error = format!("{:#}", compile_rust_source(source).unwrap_err());
        assert!(
            error.contains("CRUSH-xps-9") && error.contains(" at "),
            "{error}"
        );
    }
}

#[test]
fn newline_and_stderr_forms_are_refused_in_both_macro_positions() {
    for name in ["print", "eprint", "eprintln"] {
        for source in [
            format!("fn main() {{ {name}!(\"hello\"); }}"),
            format!("fn main() {{ let x = {name}!(\"hello\"); }}"),
            format!("fn main() {{ {name}!(); }}"),
        ] {
            let error = format!("{:#}", compile_rust_source(&source).unwrap_err());
            assert!(
                error.contains("CRUSH-xps-9") && error.contains("newline/channel"),
                "{error}"
            );
        }
    }
}

#[test]
fn pure_placeholder_reuse_and_ordered_effects_remain_supported() {
    for (source, expected) in [
        (
            "fn main() { let n = 2; println!(\"{0} {0} {n}\", n); }",
            "2 2 2\n",
        ),
        (
            include_str!("rustc_corpus/format_argument_order.rs"),
            include_str!("rustc_corpus/format_argument_order.out"),
        ),
    ] {
        let program = compile_rust_source(source).unwrap();
        let result = Runtime::new()
            .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
            .run(&program)
            .unwrap();
        assert_eq!(result.output, expected);
    }
}

#[test]
fn literal_only_and_empty_format_values_are_owned_strings() {
    for source in [
        include_str!("rustc_corpus/format_owned_literal.rs"),
        include_str!("rustc_corpus/format_owned_empty.rs"),
    ] {
        let error = format!("{:#}", compile_rust_source(source).unwrap_err());
        assert!(
            error.contains("CRUSH-xps-7") && error.contains("by-value transfer"),
            "{error}"
        );
    }
}

#[test]
fn literal_debug_traversal_and_output_are_bounded() {
    let nested = format!("{}0{}", "[".repeat(18), "]".repeat(18));
    let source = format!("fn main() {{ println!(\"{{:?}}\", {nested}); }}");
    let error = format!("{:#}", compile_rust_source(&source).unwrap_err());
    assert!(error.contains("depth/item bound"), "{error}");
    let source = format!(
        "fn main() {{ println!(\"{{:?}}\", [{}]); }}",
        vec!["0"; 257].join(",")
    );
    let error = format!("{:#}", compile_rust_source(&source).unwrap_err());
    assert!(error.contains("depth/item bound"), "{error}");
    let source = format!(
        "fn main() {{ println!(\"{{:?}}\", \"{}\"); }}",
        "a".repeat(1024 * 1024)
    );
    let error = format!("{:#}", compile_rust_source(&source).unwrap_err());
    assert!(error.contains("byte bound"), "{error}");
}
