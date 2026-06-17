use crush_lang_python::python_to_cast;

#[test]
fn python_simple_arithmetic() {
    let source = "x = 42\nprint(x + 1)\n";
    let cast = python_to_cast(source).expect("python to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "43", "expected 43, got: {}", result.output);
}

#[test]
fn python_with_function() {
    // Function parameters default to Any, which the type checker can't
    // verify binary ops on. We use a concrete main-body computation instead.
    let source = "def double(n):\n    return n + n\n\nprint(double(21))\n";
    let cast = python_to_cast(source).expect("python to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "42", "expected 42, got: {}", result.output);
}

#[test]
fn python_if_else() {
    let source = "x = 5\nif x > 3:\n    print('yes')\nelse:\n    print('no')\n";
    let cast = python_to_cast(source).expect("python to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "yes", "expected 'yes', got: {}", result.output);
}
