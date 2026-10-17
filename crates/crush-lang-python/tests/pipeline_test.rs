use crush_lang_python::python_to_cast;

#[test]
fn python_simple_arithmetic() {
    let source = "x = 42\nprint(x + 1)\n";
    let cast = python_to_cast(source).expect("python to cast");
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
fn python_with_function() {
    // Function parameters default to Any, which the type checker can't
    // verify binary ops on. We use a concrete main-body computation instead.
    let source = "def double(n):\n    return n + n\n\nprint(double(21))\n";
    let cast = python_to_cast(source).expect("python to cast");
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
fn python_if_else() {
    let source = "x = 5\nif x > 3:\n    print('yes')\nelse:\n    print('no')\n";
    let cast = python_to_cast(source).expect("python to cast");
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

/// `a if c else b` must evaluate lazily and yield the taken branch. It used
/// to lower to `__crush_ifexpr__`, an unknown helper: the runners stubbed it
/// to `None`, and recursion through it never terminated.
#[test]
fn python_conditional_expression() {
    let source = "def fib(n):\n    return n if n < 2 else fib(n - 1) + fib(n - 2)\n\nn = 7\nprint(\"big\" if n > 5 else \"small\", fib(15))\n";
    let cast = python_to_cast(source).expect("python to cast");
    let casm = crush_frontend::compiler::Compiler::new().compile(cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let result = crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None).expect("vm run");
    assert_eq!(result.output.trim(), "big 610", "got: {}", result.output);
}

/// CRUSH-111: `range()` lowers to `make_range`, a compiler-owned builtin. The
/// checked front door (`compile_cast`) used to refuse it with
/// `Undefined function: make_range` while `Compiler::new().compile` ran it.
/// Both doors must now accept the program and print the same thing.
fn run_both_doors(source: &str) -> (String, String) {
    let cast = python_to_cast(source).expect("python to cast");
    let run = |casm| {
        let vm = crush_lang_sdk::compile::casm_to_vm(casm).expect("casm to vm");
        crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)
            .expect("vm run")
            .output
    };
    let checked = crush_frontend::compile_cast(&cast).expect("compile_cast");
    let walker = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .expect("Compiler::compile");
    (run(&checked), run(&walker))
}

#[test]
fn python_range_for_loop_agrees_across_both_doors() {
    let (checked, walker) = run_both_doors("for i in range(3):\n    print(i)\n");
    assert_eq!(checked, walker);
    assert_eq!(checked.split_whitespace().collect::<Vec<_>>(), ["0", "1", "2"]);
}

#[test]
fn python_range_comprehension_agrees_across_both_doors() {
    let (checked, walker) =
        run_both_doors("xs = [i for i in range(3)]\nprint(xs[1])\n");
    assert_eq!(checked, walker);
    assert_eq!(checked.trim(), "1");
}
