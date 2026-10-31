//! CRUSH-xps-13: public SDK method contracts and registration.
#![cfg(all(feature = "polyglot-rust", feature = "stdlib"))]
use crush_lang_sdk::{HostCapsBuilder, Quotas, Runtime, Value, compile::compile_rust_source};
#[test]
fn supported_method_domains_match_native_fixtures() {
    for (source, expected) in [
        (
            include_str!("rustc_corpus/methods_unicode.rs"),
            include_str!("rustc_corpus/methods_unicode.out"),
        ),
        (
            include_str!("rustc_corpus/methods_integer.rs"),
            include_str!("rustc_corpus/methods_integer.out"),
        ),
        (
            include_str!("rustc_corpus/methods_float.rs"),
            include_str!("rustc_corpus/methods_float.out"),
        ),
        (
            include_str!("rustc_corpus/methods_nul.rs"),
            include_str!("rustc_corpus/methods_nul.out"),
        ),
    ] {
        let p = compile_rust_source(source).unwrap();
        let r = Runtime::new()
            .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
            .run(&p)
            .unwrap();
        assert_eq!(r.output, expected);
    }
}
#[test]
fn numeric_and_boolean_result_tags_and_canonical_sdk_flags_are_distinct() {
    let caps = HostCapsBuilder::new().stdlib(true).build();
    assert_eq!(
        caps.get("rust.num")
            .unwrap()
            .call(vec![
                Value::Str("pow".into()),
                Value::Str("i8".into()),
                Value::Int(6),
                Value::Int(3)
            ])
            .unwrap(),
        Some(Value::Int(-40))
    );
    let args = vec![Value::Str("λx".into()), Value::Str("λ".into())];
    assert_eq!(
        caps.get("str.starts_with")
            .unwrap()
            .call(args.clone())
            .unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        caps.get("rust.str.starts_with")
            .unwrap()
            .call(args)
            .unwrap(),
        Some(Value::Bool(true))
    );
    assert!(caps.get("rust.num").unwrap().call(vec![]).is_err());
    assert!(
        caps.get("rust.str.starts_with")
            .unwrap()
            .call(vec![Value::Int(1), Value::Int(2)])
            .is_err()
    );
    assert_eq!(
        caps.get("str.to_lower")
            .unwrap()
            .call(vec![Value::Int(42)])
            .unwrap(),
        Some(Value::Str("42".into()))
    );
}
#[test]
fn method_registration_and_host_denial_are_not_language_exceptions() {
    let p = compile_rust_source("fn main() { println!(\"{}\", 2i64.pow(3) / 3); }").unwrap();
    let missing = Runtime::new().run(&p).unwrap_err().to_string();
    assert!(
        missing.contains("rust.num") && !missing.contains("CRUSH-xps-13"),
        "{missing}"
    );
    let q = Quotas {
        allowed_caps: Some(vec!["io.print".into(), "conv.text".into()]),
        ..Default::default()
    };
    let denied = Runtime::with_quotas(q)
        .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
        .run(&p)
        .unwrap_err()
        .to_string();
    assert!(
        denied.contains("denied") && denied.contains("rust.num"),
        "{denied}"
    );
    assert!(
        HostCapsBuilder::new()
            .stdlib(false)
            .build()
            .get("rust.num")
            .is_none()
    );
}
#[test]
fn canonical_method_results_can_enter_the_shared_debug_formatter() {
    let p=compile_rust_source("fn main() { let x = 2.5f64.powi(-3); let n = 2i64.pow(3); println!(\"{:?} {:?}\", x, n); }").unwrap();
    let r = Runtime::new()
        .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
        .run(&p)
        .unwrap();
    assert_eq!(r.output, "0.064 8\n");
}

#[test]
fn unknown_indirect_and_nested_numeric_arguments_are_source_refused() {
    for source in [
        "fn root(x: f64) -> f64 { x.sqrt() } fn main() { let f = root; println!(\"{}\", f(1.0f64 / 0.0f64)); }",
        "fn roots(xs: Vec<f64>) { for x in xs { println!(\"{}\", x.sqrt()); } } fn main() { roots(vec![1.0f64 / 0.0f64]); }",
    ] {
        let e = format!("{:#}", compile_rust_source(source).unwrap_err());
        assert!(
            e.contains("CRUSH-xps-13")
                && (e.contains("argument provenance")
                    || e.contains("numeric call argument")
                    || e.contains("canonical typed arithmetic")),
            "{e}"
        );
    }
}

#[test]
fn generic_arithmetic_cannot_erase_method_widths_or_wide_comparison_precision() {
    for source in [
        "fn main() { let x = 20u8.pow(2); println!(\"{}\", x + 200); }",
        "fn main() { let x = 9007199254740992i64.pow(1); println!(\"{}\", x < 9007199254740993i64); }",
    ] {
        let e = format!("{:#}", compile_rust_source(source).unwrap_err());
        assert!(e.contains("CRUSH-xps-13"), "{e}");
    }
}
