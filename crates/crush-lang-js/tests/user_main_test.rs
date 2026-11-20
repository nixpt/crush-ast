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

fn lower_err(source: &str) -> String {
    match crush_lang_js::js_to_cast(source, "js") {
        Ok(_) => panic!("{source:?} lowered; expected a refusal"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn a_parameter_named_main_is_not_renamed() {
    let out = run(
        "function main() { return 1; }\nfunction f(main) { return main; }\nconsole.log(f(5));\nconsole.log(main());\n",
    );
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(lines, ["5", "1"], "{out:?}");
}

#[test]
fn a_local_named_main_is_not_renamed() {
    let out = run(
        "function main() { return 1; }\nfunction f() { let main = 3; return main; }\nconsole.log(f());\nconsole.log(main());\n",
    );
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    assert_eq!(lines, ["3", "1"], "{out:?}");
}

#[test]
fn top_level_code_rebinding_main_is_refused() {
    let msg = lower_err("function main() { return 1; }\nmain = 5;\nconsole.log(main);\n");
    assert!(msg.contains("rebinds `main`"), "{msg}");
}

#[test]
fn a_taken_reserved_name_is_refused() {
    let msg = lower_err(
        "function __user_main() { return 2; }\nfunction main() { return 1; }\nconsole.log(main());\n",
    );
    assert!(msg.contains("__user_main"), "{msg}");
}
