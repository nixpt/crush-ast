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
