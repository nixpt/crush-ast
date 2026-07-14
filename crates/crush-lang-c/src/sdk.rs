/// End-to-end test: walk a C program that calls __crush_ffi__, run it in CVM1.
///
/// Requires the example C plugin to be pre-built by crush-vm's build.rs.
#[test]
fn test_c_walker_ffi_call() {
    let plugin_so = std::path::PathBuf::from(env!("EXAMPLE_C_PLUGIN_SO"));
    if !plugin_so.exists() {
        eprintln!("Skipping: example_c_plugin.so not built");
        return;
    }

    let plugin_path = plugin_so.to_string_lossy().replace('\\', "\\\\");

    // C source that calls __crush_ffi__("math.add", 10, 32)
    let source = format!(
        r#"int main() {{
    __crush_ffi__("{plugin_path}", "math.add", 10, 32);
    return 0;
}}"#
    );

    // Walk C → CAST
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(&source, None).unwrap();
    let walker = CWalker {
        file_name: "test_ffi.c".to_string(),
    };
    let cast = walker.walk(&tree, source.as_bytes()).unwrap();

    // CAST → CASM
    let mut compiler = crush_frontend::compiler::Compiler::new();
    let casm = compiler.compile(cast).unwrap();

    // CASM → CVM1 program
    let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm).unwrap();

    // Register capabilities
    let mut host_caps = crush_vm::host::HostCaps::new();
    host_caps.register(Box::new(crush_vm::plugin::FfiGatewayCap));

    // Register io.print as nop (needed for printf mapping)
    struct PrintCap;
    impl crush_vm::host::HostCap for PrintCap {
        fn spec(&self) -> crush_vm::host::HostCapSpec {
            crush_vm::host::HostCapSpec {
                name: "io.print".into(),
                argc: None,
                returns: false,
            }
        }
        fn call(
            &self,
            _args: Vec<crush_vm::vm::Value>,
        ) -> Result<Option<crush_vm::vm::Value>, String> {
            Ok(None)
        }
    }
    host_caps.register(Box::new(PrintCap));

    let quotas = crush_vm::vm::Quotas {
        max_steps: 10_000_000,
        ..Default::default()
    };

    let result = crush_vm::vm::run_with_caps(&vm_prog, &quotas, Some(&host_caps)).unwrap();
    // The FFI call's return value should be on the stack (42 = 10 + 32)
    // plus null from the ExprStmt pop
    assert!(
        result.stack.iter().any(|v| matches!(v, crush_vm::vm::Value::Int(42))),
        "Expected Int(42) on stack, got: {:?}",
        result.stack
    );
}
