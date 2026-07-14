//! C SDK — full pipeline from C source to execution.
//!
//! Provides the complete bridge: walk → CAST → CASM → CVM1 execution.
//!
//! ```rust,no_run
//! let result = crush_lang_c::sdk::run_c("int main() { return 42; }", "test.c")?;
//! assert_eq!(result, "42");
//! ```

use anyhow::Result;
use tree_sitter::Parser;
use walker_core::Walker;

use crate::CWalker;

/// Walk C source → CAST AST.
///
/// Auto-detects C vs C++ based on filename extension.
pub fn c_to_cast(source: &str, filename: &str) -> Result<crush_cast::Program> {
    let is_cpp = filename.ends_with(".cpp")
        || filename.ends_with(".cc")
        || filename.ends_with(".cxx")
        || filename.ends_with(".hpp");

    let mut parser = Parser::new();
    let lang = if is_cpp {
        tree_sitter_cpp::LANGUAGE.into()
    } else {
        tree_sitter_c::LANGUAGE.into()
    };
    parser.set_language(&lang)?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("C parse failed"))?;

    let walker = CWalker {
        file_name: filename.to_string(),
    };
    walker.walk(&tree, source.as_bytes())
}

/// CAST → CASM bytecode.
pub fn cast_to_casm(program: &crush_cast::Program) -> Result<casm::Program> {
    let mut compiler = crush_frontend::compiler::Compiler::new();
    compiler
        .compile(program.clone())
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))
}

/// CASM → CVM1 executable (thin wrapper around crush-lang-sdk).
pub fn casm_to_vm(program: &casm::Program) -> Result<crush_vm::Program> {
    crush_lang_sdk::compile::casm_to_vm(program)
        .map_err(|e| anyhow::anyhow!("CASM→CVM1: {e}"))
}

/// Full pipeline: C source → CVM1 execution → result string.
///
/// ```rust,no_run
/// let output = crush_lang_c::sdk::run_c("int main() { return 2 + 3 * 4; }", "test.c")?;
/// assert_eq!(output, "14");
/// ```
pub fn run_c(source: &str, filename: &str) -> Result<String> {
    let cast = c_to_cast(source, filename)?;
    let casm = cast_to_casm(&cast)?;
    let vm_prog = casm_to_vm(&casm)?;

    // Register io.print and other common capabilities
    use crush_vm::{HostCap, HostCapSpec, HostCaps};
    let mut host_caps = HostCaps::new();

    struct PrintCap;
    impl HostCap for PrintCap {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec { name: "io.print".into(), argc: None, returns: false }
        }
        fn call(&self, args: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
            for a in &args { print!("{a}"); } println!();
            Ok(None)
        }
    }

    struct NopCap { name: String }
    impl HostCap for NopCap {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec { name: self.name.clone(), argc: None, returns: true }
        }
        fn call(&self, _args: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
            Ok(Some(crush_vm::vm::Value::Null))
        }
    }

    host_caps.register(Box::new(PrintCap));
    for name in &[
        "append", "push", "make_range", "arr_set", "arr_get",
        "str.concat",
        "__crush_deref__", "__crush_addr_of__", "__crush_unary__",
    ] {
        host_caps.register(Box::new(NopCap { name: name.to_string() }));
    }

    let quotas = crush_vm::Quotas {
        max_steps: 10_000_000,
        ..Default::default()
    };

    let result = crush_vm::run_with_caps(&vm_prog, &quotas, Some(&host_caps))
        .map_err(|e| anyhow::anyhow!("CVM1 execution: {e}"))?;

    Ok(result.output.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        let src = "int main() { printf(2 + 3 * 4); return 0; }";
        let result = run_c(src, "test.c").unwrap();
        assert_eq!(result, "14");
    }

    #[test]
    fn test_variable_and_if() {
        let src = "int main() { int x = 5; if (x > 3) { printf(100); } else { printf(0); } return 0; }";
        let result = run_c(src, "test.c").unwrap();
        assert_eq!(result, "100");
    }

    #[test]
    fn test_for_loop_sum() {
        let src =
            "int main() { int sum = 0; for (int i = 0; i < 10; i++) { sum = sum + i; } printf(sum); return 0; }";
        let result = run_c(src, "test.c").unwrap();
        assert_eq!(result, "45");
    }

    #[test]
    fn test_while_loop() {
        let src = "int main() { int i = 0; while (i < 5) { i = i + 1; } printf(i); return 0; }";
        let result = run_c(src, "test.c").unwrap();
        assert_eq!(result, "5");
    }

    #[test]
    fn test_switch_case() {
        let src = "int main() { int x = 2; int r = 0; switch (x) { case 1: r = 100; break; case 2: r = 200; break; default: r = 0; break; } printf(r); return 0; }";
        let result = run_c(src, "test.c").unwrap();
        assert_eq!(result, "200");
    }
}
