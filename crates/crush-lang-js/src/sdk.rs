//! JS SDK — full pipeline from JavaScript/TypeScript source to CVM1 execution.
//! (Test-only: requires crush-vm/crush-frontend/crush-lang-sdk as dev-deps)
#![cfg(test)]

/// JS/TS source -> CVM1 execution -> output string.
pub fn run_js(source: &str, ext: &str) -> anyhow::Result<String> {
    let cast = crate::js_to_cast(source, ext)
        .map_err(|e| anyhow::anyhow!("JS->CAST: {e}"))?;

    let mut compiler = crush_frontend::compiler::Compiler::new();
    let casm = compiler
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST->CASM: {e}"))?;

    let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm)
        .map_err(|e| anyhow::anyhow!("CASM->CVM1: {e}"))?;

    use crush_vm::host::{HostCap, HostCapSpec, HostCaps};
    let mut host_caps = HostCaps::new();

    struct NopCap { name: String }
    impl HostCap for NopCap {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec { name: self.name.clone(), argc: None, returns: true }
        }
        fn call(&self, _: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
            Ok(Some(crush_vm::vm::Value::Null))
        }
    }
    for name in &[
        "append", "push", "make_range", "arr_set", "arr_get", "str.concat",
        "__crush_assign__", "__crush_setindex__",
        "__crush_not__", "__crush_neg__", "__crush_subscript__",
        "__crush_unary__",
    ] {
        host_caps.register(Box::new(NopCap { name: name.to_string() }));
    }

    let quotas = crush_vm::vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    let result = crush_vm::vm::run_with_caps(&vm_prog, &quotas, Some(&host_caps))
        .map_err(|e| anyhow::anyhow!("CVM1: {e}"))?;
    Ok(result.output.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        assert_eq!(run_js("2 + 3 * 4", "js").unwrap(), "");
        // ExprStmt pop means no output; just verify no crash
    }

    #[test]
    fn test_console_log() {
        assert_eq!(run_js("console.log(2 + 3 * 4)", "js").unwrap(), "14");
    }

    #[test]
    fn test_variable_and_if() {
        let src = "var x = 5; if (x > 3) { console.log(100); } else { console.log(0); }";
        assert_eq!(run_js(src, "js").unwrap(), "100");
    }

    #[test]
    fn test_while_loop() {
        let src = "var i = 0; while (i < 5) { i = i + 1; } console.log(i);";
        assert_eq!(run_js(src, "js").unwrap(), "5");
    }

    #[test]
    fn test_for_loop() {
        let src = "var sum = 0; for (var i = 0; i < 10; i++) { sum = sum + i; } console.log(sum);";
        assert_eq!(run_js(src, "js").unwrap(), "45");
    }

    #[test]
    fn test_function() {
        let src = "function add(a, b) { return a + b; } console.log(add(10, 20));";
        assert_eq!(run_js(src, "js").unwrap(), "30");
    }

    #[test]
    fn test_array_basics() {
        let src = "var arr = []; arr.push(10); arr.push(20); console.log(arr[0] + arr[1]);";
        assert_eq!(run_js(src, "js").unwrap(), "30");
    }

    #[test]
    fn test_array_set() {
        let src = "var arr = [0, 0, 0]; arr[1] = 99; console.log(arr[1]);";
        assert_eq!(run_js(src, "js").unwrap(), "99");
    }

    #[test]
    fn test_sieve_simple() {
        // Known limitation: JS walker for-loop scoping across nested loops
        // uses while-loops instead which work correctly
        let src = "var arr=[]; var i=0; while(i<10){arr.push(i*2); i=i+1;} console.log(arr[3]);";
        assert_eq!(run_js(src, "js").unwrap(), "6");
    }

    #[test]
    fn test_typescript_strips_types() {
        let src: &str = "const x: number = 42; const y: string = 'hello'; console.log(x);";
        // SWC strips TS types, JS execution continues
        assert_eq!(run_js(src, "ts").unwrap(), "42");
    }
}
/// Multi-file polyglot compilation: both walkers produce CAST, merge functions, compile.
/// Proves the Mallika dream: ONE binary from multiple source languages.
#[test]
fn test_polyglot_merge_cast() {
    let js_src = "function add(a, b) { return a + b; }";
    let py_src = "def multiply(x, y):\n    return x * y";

    // Walk JS → CAST
    let mut js_cast = crate::js_to_cast(js_src, "js").unwrap();
    assert!(js_cast.functions.contains_key("add"));

    // Walk Python → CAST (requires crush-lang-python dev-dep)
    let py_cast = crush_lang_python::python_to_cast(py_src).unwrap();
    assert!(py_cast.functions.contains_key("multiply"));

    // Merge: the killer feature. Two languages, one CAST, one binary.
    js_cast.functions.extend(py_cast.functions);
    assert!(js_cast.functions.contains_key("add"));
    assert!(js_cast.functions.contains_key("multiply"));

    // Compile merged CAST → CASM
    let mut compiler = crush_frontend::compiler::Compiler::new();
    let casm = compiler.compile(js_cast).unwrap();
    assert!(casm.functions.contains_key("add"), "JS fn should compile");
    assert!(casm.functions.contains_key("multiply"), "Python fn should compile");
}
