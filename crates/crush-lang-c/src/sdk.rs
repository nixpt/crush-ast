//! C SDK — full pipeline tools for C source in Crush.
//!
//! Requires `crush-frontend`, `crush-lang-sdk`, `crush-vm`, `casm` as
//! dev-dependencies in `Cargo.toml`.

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::CWalker;
    use tree_sitter::Parser;
    use walker_core::Walker;

    /// Walk C source → CAST → CASM → CVM1 → stack.
    fn run_c(source: &str) -> Result<String, String> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .map_err(|e| format!("lang init: {e}"))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| "parse failed".to_string())?;
        let walker = CWalker {
            file_name: "test.c".to_string(),
        };
        let cast = walker
            .walk(&tree, source.as_bytes())
            .map_err(|e| format!("walk: {e}"))?;

        let mut compiler = crush_frontend::compiler::Compiler::new();
        let casm = compiler.compile(cast).map_err(|e| format!("compile: {e}"))?;
        let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm)
            .map_err(|e| format!("asm: {e}"))?;

        use crush_vm::host::{HostCap, HostCapSpec, HostCaps};
        let mut host_caps = HostCaps::new();

        struct PrintCap;
        impl HostCap for PrintCap {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec { name: "io.print".into(), argc: None, returns: false }
            }
            fn call(&self, _args: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
                Ok(None)
            }
        }
        host_caps.register(Box::new(PrintCap));

        // __crush_ffi__ bridge
        host_caps.register(Box::new(crush_vm::plugin::FfiGatewayCap));

        struct NopCap { name: String }
        impl HostCap for NopCap {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec { name: self.name.clone(), argc: None, returns: true }
            }
            fn call(&self, _args: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
                Ok(Some(crush_vm::vm::Value::Null))
            }
        }
        for name in &[
            "__crush_assign__", "__crush_deref__", "__crush_addr_of__",
            "__crush_not__", "__crush_bit_not__", "__crush_neg__",
            "__crush_pos__", "__crush_subscript__", "__crush_ternary__",
            "__crush_pre_inc__", "__crush_post_inc__",
            "__crush_pre_dec__", "__crush_post_dec__",
            "__crush_unary__", "__crush_setindex__",
        ] {
            host_caps.register(Box::new(NopCap { name: name.to_string() }));
        }

        let quotas = crush_vm::vm::Quotas { max_steps: 10_000_000, ..Default::default() };
        let result = crush_vm::vm::run_with_caps(&vm_prog, &quotas, Some(&host_caps))
            .map_err(|e| format!("exec: {e}"))?;
        Ok(format!("{:?}", result.stack))
    }

    #[test]
    fn test_simple_c_via_cvm() {
        let src = "int main() { int x = 2 + 3 * 4; printf(x); return 0; }";
        // Debug: dump CAST
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let walker = CWalker { file_name: "test.c".to_string() };
        let cast = walker.walk(&tree, src.as_bytes()).unwrap();
        eprintln!("CAST main body: {:#?}", cast.functions.get("main").map(|f| &f.body));
        let result = run_c(src).unwrap();
        assert!(result.contains("Int(14)"), "Expected Int(14), got: {result}");
    }

    #[test]
    fn test_c_ffi_call_cast_lowering() {
        // Verify the walker lowers __crush_ffi__ to a CapabilityCall
        let source = "int main() { __crush_ffi__(\"/tmp/test.so\", \"math.add\", 10, 32); return 0; }";
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let walker = CWalker { file_name: "test.c".to_string() };
        let cast = walker.walk(&tree, source.as_bytes()).unwrap();

        let main = cast.functions.get("main").unwrap();
        assert_eq!(main.body.len(), 2, "expected 2 stmts: ffi call + return");

        let ffi_stmt = &main.body[0];
        if let crush_cast::Statement::ExprStmt { expr, .. } = ffi_stmt {
            if let crush_cast::Expression::CapabilityCall { name, args, .. } = expr {
                assert_eq!(name, "__crush_ffi__");
                assert_eq!(args.len(), 4, "lib_path + cap_name + 2 args = 4");
            } else {
                panic!("Expected CapabilityCall, got {:?}", expr);
            }
        } else {
            panic!("Expected ExprStmt, got {:?}", ffi_stmt);
        }
    }

    #[test]
    fn test_c_ffi_full_pipeline() {
        // Build the example plugin if gcc is available
        let out_dir = std::env::temp_dir().join("crush-lang-c-test");
        let _ = std::fs::create_dir_all(&out_dir);
        let so_path = out_dir.join("example_c_plugin.so");
        let plugin_src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crush-ffi/examples/example_c_plugin.c");
        let include_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crush-ffi/include");

        if !plugin_src.exists() || !include_dir.exists() {
            eprintln!("Skipping full pipeline: plugin source not found");
            return;
        }

        let status = std::process::Command::new("gcc")
            .args([
                "-shared", "-fPIC", "-std=c11", "-O2",
                "-o", so_path.to_str().unwrap(),
                plugin_src.to_str().unwrap(),
                "-I", include_dir.to_str().unwrap(),
            ])
            .status();
        let status = match status {
            Ok(s) => s,
            Err(_) => { eprintln!("Skipping: gcc not found"); return; }
        };
        if !status.success() {
            eprintln!("Skipping: gcc compilation failed");
            return;
        }

        let path = so_path.to_string_lossy().replace('\\', "/");
        let source = format!(
            "int main() {{ __crush_ffi__(\"{path}\", \"math.add\", 10, 32); return 0; }}"
        );

        let result = run_c(&source).unwrap();
        assert!(
            result.contains("Int(42)"),
            "Expected Int(42) from math.add(10,32), got: {result}"
        );
    }
}
