//! CRUSH-560: a user function named `main` is an ordinary function. It must
//! not replace the module body (top-level code and the `main()` call).
#![cfg(feature = "swc-backend")]

fn run(source: &str) -> String {
    let cast = crush_lang_js::js_to_cast(source, "js").expect("js to cast");
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
        "console.log('before');\nfunction main() { console.log('in main'); return 7; }\nconsole.log('after');\nconsole.log(main());\n",
    );
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(lines, ["before", "after", "in main", "7"], "{out:?}");
}

#[test]
fn user_main_that_is_never_called_does_not_run() {
    let out = run("function main() { console.log('main'); }\nconsole.log('top');\n");
    assert_eq!(out.trim(), "top");
}

#[test]
fn user_main_can_recurse() {
    let out = run(
        "function main(n) { if (n === 0) { return 0; } return n + main(n - 1); }\nconsole.log(main(4));\n",
    );
    assert_eq!(out.trim(), "10");
}
