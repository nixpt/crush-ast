use crush_lang_rust::rust_to_cast;

#[test]
fn rust_simple_arithmetic() {
    let source = "fn main() { let x = 42; println(x + 1); }";
    let cast = rust_to_cast(source).expect("rust to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(
        result.output.trim(),
        "43",
        "expected 43, got: {}",
        result.output
    );
}

#[test]
fn rust_with_function() {
    let source = "fn double(n: i32) -> i32 { return n + n; } fn main() { println(double(21)); }";
    let cast = rust_to_cast(source).expect("rust to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(
        result.output.trim(),
        "42",
        "expected 42, got: {}",
        result.output
    );
}

#[test]
fn rust_if_else() {
    let source = "fn main() { let x = 5; if x > 3 { println(\"yes\"); } else { println(\"no\"); } }";
    let cast = rust_to_cast(source).expect("rust to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(
        result.output.trim(),
        "yes",
        "expected 'yes', got: {}",
        result.output
    );
}
