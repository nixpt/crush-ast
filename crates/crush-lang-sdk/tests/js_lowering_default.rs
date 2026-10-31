//! CRUSH-250: until step 6 flips the default, the SDK's `polyglot-javascript`
//! must keep `.js` on the swc lowering, which passes the parity corpus. The
//! boa lowering is compiled in too (for `@javascript {}` analysis), and must
//! not take over `.js` early.

#![cfg(feature = "polyglot-javascript")]

#[test]
fn js_walks_through_the_swc_lowering() {
    assert_eq!(crush_lang_js::JsLowering::for_ext("js").unwrap(), crush_lang_js::JsLowering::Swc);
}

#[test]
fn a_js_program_runs_through_the_default_sdk() {
    let source = "let total = 0;\nfor (let i = 1; i <= 4; i++) { total += i; }\nconsole.log('total', total);\n";
    let cast = crush_lang_js::js_to_cast(source, "js").expect("walk");
    let casm = crush_frontend::compiler::Compiler::new().compile(cast).expect("compile");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("assemble");
    let caps = crush_lang_sdk::HostCapsBuilder::new().stdlib(true).build();
    let out = crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), Some(&caps)).expect("run");
    assert_eq!(out.output, "total 10\n");
}
