//! CRUSH-260: `x.length` on strings and arrays.
//!
//! It used to lower to a field read and yield `null`, so
//! `total = total + s.length` failed with "arithmetic on non-numeric operands".
//! It now lowers to the `len` op, which every engine (incl. AOT) implements.

use crush_lang_js::js_to_cast;

fn run_js(source: &str) -> String {
    let cast = js_to_cast(source, "js").expect("js to cast");
    // The walker pipeline's compile step (crush-walk-run / crush-aotc): it
    // understands the walker's lowering helpers (`__crush_assign__`,
    // `make_range`) that `compile_cast`'s semantic check rejects.
    let casm = crush_frontend::compiler::Compiler::new().compile(cast).expect("cast to casm");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm");
    let quotas = crush_vm::Quotas::default();
    let caps = crush_lang_sdk::HostCapsBuilder::new().stdlib(true).build();
    let result = crush_vm::run_with_caps(&vm, &quotas, Some(&caps)).expect("vm run");
    result.output.trim().to_string()
}

#[test]
fn string_and_array_length() {
    let out = run_js(
        r#"
let s = "hello";
let a = [1, 2, 3];
console.log(s.length);
console.log(a.length);
let total = 0;
let i = 0;
while (i < 3) { total = total + ("x" + i).length; i = i + 1; }
console.log(total);
"#,
    );
    assert_eq!(out, "5\n3\n6");
}

#[test]
fn length_in_loop_condition() {
    let out = run_js(
        r#"
let a = [4, 5, 6];
let sum = 0;
for (let i = 0; i < a.length; i++) { sum = sum + a[i]; }
console.log(sum);
"#,
    );
    assert_eq!(out, "15");
}

#[test]
fn object_literal_keeps_its_length_field() {
    let out = run_js("console.log(({length: 3}).length);");
    assert_eq!(out, "3");
}

#[test]
fn optional_chain_length() {
    let out = run_js("let s = \"abcd\"; console.log(s?.length);");
    assert_eq!(out, "4");
}
