//! CRUSH-560: a user `def main` is an ordinary function. It must not replace
//! the module body (top-level code and the `main()` call).
use crush_lang_python::python_to_cast;

fn run(source: &str) -> String {
    let cast = python_to_cast(source).expect("python to cast");
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .expect("CAST to CASM");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)
        .expect("vm run")
        .output
}

#[test]
fn user_main_does_not_replace_the_module_body() {
    let out = run(
        "print('before')\ndef main():\n    print('in main')\n    return 7\nprint('after')\nprint(main())\n",
    );
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(lines, ["before", "after", "in main", "7"], "{out:?}");
}

#[test]
fn user_main_that_is_never_called_does_not_run() {
    let out = run("def main():\n    print('main')\nprint('top')\n");
    assert_eq!(out.trim(), "top");
}

#[test]
fn user_main_can_recurse() {
    let out = run(
        "def main(n):\n    if n == 0:\n        return 0\n    return n + main(n - 1)\nprint(main(4))\n",
    );
    assert_eq!(out.trim(), "10");
}

fn lower_err(source: &str) -> String {
    match python_to_cast(source) {
        Ok(_) => panic!("{source:?} lowered; expected a refusal"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn a_parameter_named_main_is_not_renamed() {
    let out = run(
        "def main():\n    return 1\ndef f(main):\n    return main\nprint(f(5))\nprint(main())\n",
    );
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(lines, ["5", "1"], "{out:?}");
}

#[test]
fn a_local_named_main_is_not_renamed() {
    let out = run(
        "def main():\n    return 1\ndef f():\n    main = 3\n    return main\nprint(f())\nprint(main())\n",
    );
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(lines, ["3", "1"], "{out:?}");
}

#[test]
fn top_level_code_rebinding_main_is_refused() {
    let msg = lower_err("def main():\n    return 1\nmain = 5\nprint(main)\n");
    assert!(msg.contains("rebinds `main`"), "{msg}");
}

#[test]
fn a_taken_reserved_name_is_refused() {
    let msg =
        lower_err("def __user_main():\n    return 2\ndef main():\n    return 1\nprint(main())\n");
    assert!(msg.contains("__user_main"), "{msg}");
}
