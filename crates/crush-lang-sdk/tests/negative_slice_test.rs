//! #254: negative slice bounds count from the end, as `a[-1]` does. Both VMs
//! used to clamp them to 0, so `a[1:-1]` was `[]` and `a[-3:]` the whole array.

use crush_vm::{Quotas, run};

#[test]
fn negative_slice_bounds_count_from_the_end() {
    let src = "let a = [1, 2, 3, 4]\nprint(a[1:-1])\nprint(a[-3:])\nprint(a[:-1])\nprint(a[-1])\n";
    let casm = crush_frontend::compile_crush_source(src).expect("compile");
    let program = crush_lang_sdk::compile::casm_to_vm(&casm).expect("lower");
    let out = run(&program, &Quotas::default()).expect("run").output;
    assert_eq!(out, "[2, 3]\n[2, 3, 4]\n[1, 2, 3]\n4\n");
}
