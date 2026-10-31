//! Shared Debug kernel integration; C refuses before producing an artifact.
use crush_aot::{AotCompiler, Module};
use crush_vm::RuntimeValue;

#[test]
fn rust_aot_calls_the_embedded_kernel_once_and_returns_typed_text() {
    let source = r#"fn main() { return rust.debug("é", "{\"kind\":\"char\"}"); }"#;
    let path = AotCompiler::new()
        .compile_source(source, "typed_debug_char")
        .unwrap();
    let module = Module::load(path).unwrap();
    assert!(matches!(module.call_main().unwrap(), RuntimeValue::String(s) if s == "'é'"));
}
#[test]
fn rust_aot_embeds_the_vm_kernel_and_c_has_an_explicit_bridge_boundary() {
    let ast = crush_frontend::parse_source(
        r#"fn main() { return rust.debug("x", "{\"kind\":\"str\"}"); }"#,
    )
    .unwrap();
    let casm = crush_frontend::compile_cast_owned(ast).unwrap();
    let rust = crush_aot::codegen::gen_rust_source(&casm).unwrap();
    assert!(rust.contains(crush_vm::rust_debug::CORE_SOURCE));
    let error = crush_aot::codegen_c::gen_c_source(&casm)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("shared Rust formatter bridge is required") && error.contains("CRUSH-xps-9"),
        "{error}"
    );
}
