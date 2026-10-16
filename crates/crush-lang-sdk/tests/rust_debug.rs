//! xps-9 shared formatter, source descriptors and explicit stdlib registration.
#![cfg(all(feature = "polyglot-rust", feature = "stdlib"))]
use crush_lang_sdk::{
    HostCapsBuilder, Quotas, Runtime,
    compile::{compile_rust_source, compile_source},
};

#[test]
fn source_dynamic_debug_matches_native_oracles() {
    for (source, expected) in [
        (
            include_str!("rustc_corpus/debug_dynamic.rs"),
            include_str!("rustc_corpus/debug_dynamic.out"),
        ),
        (
            include_str!("rustc_corpus/float_display.rs"),
            include_str!("rustc_corpus/float_display.out"),
        ),
        (
            include_str!("rustc_corpus/debug_runtime_scalars.rs"),
            include_str!("rustc_corpus/debug_runtime_scalars.out"),
        ),
        (
            include_str!("rustc_corpus/debug_runtime_sequences.rs"),
            include_str!("rustc_corpus/debug_runtime_sequences.out"),
        ),
        (
            include_str!("rustc_corpus/debug_runtime_float.rs"),
            include_str!("rustc_corpus/debug_runtime_float.out"),
        ),
    ] {
        let program = compile_rust_source(source).unwrap();
        assert!(
            program
                .manifest
                .permissions
                .iter()
                .any(|p| p == "rust.debug")
        );
        let result = Runtime::new()
            .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
            .run(&program)
            .unwrap();
        assert_eq!(result.output, expected);
    }
}
#[test]
fn registration_and_allowlist_denials_are_distinct_from_language_refusals() {
    let program = compile_rust_source(include_str!("rustc_corpus/debug_dynamic.rs")).unwrap();
    let error = Runtime::new().run(&program).unwrap_err().to_string();
    assert!(error.contains("rust.debug"), "{error}");
    assert!(!error.contains("CRUSH-xps-9"), "{error}");
    let quotas = Quotas {
        allowed_caps: Some(vec!["io.print".into(), "conv.text".into()]),
        ..Default::default()
    };
    let error = Runtime::with_quotas(quotas)
        .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
        .run(&program)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("denied") && error.contains("rust.debug"),
        "{error}"
    );
    let caps = HostCapsBuilder::new().stdlib(true).build();
    assert_eq!(caps.get("rust.debug").unwrap().effects(), Some(&[][..]));
    assert!(
        HostCapsBuilder::new()
            .stdlib(false)
            .build()
            .get("rust.debug")
            .is_none()
    );
}
#[test]
fn direct_stdlib_calls_validate_descriptors_and_runtime_values() {
    for source in [
        r#"fn main() { return rust.debug(256, "{\"kind\":\"u8\"}"); }"#,
        r#"fn main() { return rust.debug("many", "{\"kind\":\"char\"}"); }"#,
        r#"fn main() { return rust.debug(1, "{\"kind\":\"f64\"}"); }"#,
        r#"fn main() { return rust.debug(1, "{\"kind\":\"custom\"}"); }"#,
    ] {
        let program = compile_source("crush", source).unwrap();
        let error = Runtime::new()
            .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
            .run(&program)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("rust.debug") && error.contains("CRUSH-xps-9"),
            "{error}"
        );
    }
}
#[test]
fn source_types_are_not_guessed_from_annotation_or_runtime_string_shape() {
    for source in [
        "fn main() { let v = vec![1, 2]; println!(\"{:?}\", v.iter()); }",
        "fn main() { let x = 1.1f64.powi(3); println!(\"{:?}\", x); }",
        "fn main() { let a = 9007199254740992i64; let b = 9007199254740993i64; println!(\"{:?}\", a < b); }",
        "fn pick(a: i64, b: i64) -> char { if a < b { 'x' } else { 'y' } } fn main() { println!(\"{:?}\", pick(9007199254740992, 9007199254740993)); }",
        "fn main() { let chars = ['x','y']; let i: usize = 0; println!(\"{:?}\", chars[i + 1]); }",
        "fn len(n: i64) -> i64 { n } fn main() { println!(\"{:?}\", len(7)); }",
        "fn print(n: i64) -> i64 { n } fn main() { println!(\"{:?}\", print(7)); }",
        "fn tick(n: i64) -> i64 { println!(\"{}\", n); n } fn pair(a: i64, b: i64) -> i64 { a } fn main() { println!(\"{:?}\", pair(tick(1), tick(2))); }",
        "fn main() { let x: f32 = 1.0; println!(\"{:?}\", x); }",
        "fn show(c: char) { println!(\"{:?}\", c); } fn main() { show(\"x\"); }",
        "fn main() { let n = 2i64.pow(3); println!(\"{:?}\", n); }",
        "fn main() { let a = vec![1]; let mut b = a; b.push(2); println!(\"{:?}\", b); }",
        "fn show(n: usize) { println!(\"{:?}\", n); } fn main() { show(70000); }",
    ] {
        let error = format!("{:#}", compile_rust_source(source).unwrap_err());
        assert!(
            error.contains("CRUSH-xps-9") && error.contains(" at "),
            "{error}"
        );
    }
}
#[test]
fn nested_format_results_keep_owned_string_identity() {
    let source =
        "fn main() { let n = 3; let s = format!(\"{:?}\", n); let t = s; println!(\"{}\", t); }";
    let error = format!("{:#}", compile_rust_source(source).unwrap_err());
    assert!(
        error.contains("CRUSH-xps-7") && error.contains("by-value transfer"),
        "{error}"
    );
}
