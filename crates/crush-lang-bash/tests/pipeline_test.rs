use crush_lang_bash::bash_to_cast;

#[test]
fn bash_simple_echo() {
    let source = "echo 42\n";
    let cast = bash_to_cast(source).expect("bash to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "42", "expected 42, got: {}", result.output);
}

#[test]
fn bash_variable_echo() {
    let source = "x=42\necho $x\n";
    let cast = bash_to_cast(source).expect("bash to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "42", "expected 42, got: {}", result.output);
}

#[test]
fn bash_function_call() {
    let source = "f() { echo 42; }\nf\n";
    let cast = bash_to_cast(source).expect("bash to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "42", "expected 42, got: {}", result.output);
}

#[test]
fn bash_if_true() {
    let source = "if true; then echo yes; else echo no; fi\n";
    let cast = bash_to_cast(source).expect("bash to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "yes", "expected 'yes', got: {}", result.output);
}

#[test]
fn bash_if_false() {
    let source = "if false; then echo yes; else echo no; fi\n";
    let cast = bash_to_cast(source).expect("bash to cast");
    let casm = crush_frontend::compile_cast(&cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run_with_caps(&vm, &quotas, None).expect("vm run");
    assert_eq!(result.output.trim(), "no", "expected 'no', got: {}", result.output);
}
