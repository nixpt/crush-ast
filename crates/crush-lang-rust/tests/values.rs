//! The value pass must finish before CAST reaches the shared compiler.
use crush_lang_rust::rust_to_cast;

#[test]
fn sequence_clones_lower_to_fresh_slice_capabilities() {
    let cast = rust_to_cast("fn main() { let a = vec![1]; let b = a.clone().clone(); }").unwrap();
    let json = serde_json::to_string(&cast).unwrap();
    assert_eq!(json.matches("seq.slice").count(), 2, "{json}");
    assert!(!json.contains("__crush_rust_value__"), "{json}");
}

#[test]
fn identity_adapters_leave_no_private_runtime_helper() {
    let cast = rust_to_cast(
        r#"fn main() {
        let s = "hi".to_string(); let t = s.clone() + "!";
        let a = vec![1, 2]; for n in a.iter() { println!("{}", n); }
        let b = a.as_slice().to_vec();
    }"#,
    )
    .unwrap();
    let json = serde_json::to_string(&cast).unwrap();
    assert!(!json.contains("__crush_rust_value__"), "{json}");
    assert!(json.contains("conv.text") && json.contains("seq.slice"));
}

#[test]
fn source_functions_cannot_be_mistaken_for_value_intrinsics() {
    rust_to_cast(r#"
        fn __crush_rust_value__(x: i64) -> i64 { x }
        fn __crush_ternary__(x: i64, y: i64, z: i64) -> i64 { x + y + z }
        fn __crush_assign__(x: i64, y: i64) -> i64 { x + y }
        fn main() { println!("{}", __crush_rust_value__(__crush_assign__(1, __crush_ternary__(2, 3, 4)))); }
    "#).unwrap();
}

#[test]
fn unproven_transfers_are_rejected_with_their_location() {
    for body in [
        "let a = vec![1]; let b = a;",
        "let a = [1]; let b = a;",
        "let a = vec![1]; let b = match true { true => a, false => vec![2] };",
        "let a = vec![1]; let b = if true { a } else { vec![2] };",
        "let a = vec![1]; for n in a.into_iter() {}",
        "let a = vec![1]; let b = a.as_slice();",
        "let a = vec![1]; let b = a.iter().to_owned();",
        "let a = vec![1]; for n in a.iter() { let x = *n; }",
        "let a = vec![1]; let b = &a;",
        "let a = vec![vec![1]]; let b = a.clone();",
        "let a = vec![1]; let b = || a.clone();",
        "let mut a = vec![1]; let b = || { a = vec![2]; };",
        "let a = \"hi\".to_string(); let b = a + \"!\";",
        "let mut a = vec![1]; let b = a.pop().unwrap();",
    ] {
        let error = rust_to_cast(&format!("fn main() {{\n    {body}\n}}")).unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("Rust value semantics at 2:") && error.contains("CRUSH-xps-7"),
            "{body}: {error}"
        );
    }
}

#[test]
fn borrowed_results_cannot_escape_through_arguments_or_returns() {
    for source in [
        "fn size(s: &str) -> usize { s.len() } fn main() { let s = \"hi\".to_string(); size(s.as_str()); }",
        "fn bad(s: String) -> &str { s.as_str() } fn main() {}",
        "fn unknown<T: Clone>(v: T) { v.clone(); } fn main() {}",
        "fn bad(s: &String) {} fn main() {}",
    ] {
        let error = format!("{:#}", rust_to_cast(source).unwrap_err());
        assert!(
            error.contains("Rust value semantics at") && error.contains("CRUSH-xps-7"),
            "{error}"
        );
    }
}

#[test]
fn scalar_captures_and_borrowed_str_parameters_stay_supported() {
    rust_to_cast(r#"
        fn size(s: &str) -> usize { s.len() }
        fn main() { let mut n = 0; let mut f = || { n += 1; n }; println!("{} {}", f(), size("hi")); }
    "#).unwrap();
}

#[test]
fn assignments_have_a_unit_result_and_keep_the_targets_owned_type() {
    rust_to_cast(
        r#"
        fn accept(_: ()) {}
        fn main() {
            let a = vec![1]; let mut b = vec![2];
            accept(b = a.clone());
            let c = b.clone();
        }
    "#,
    )
    .unwrap();
}
