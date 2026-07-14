//! Python SDK — full pipeline from Python source to CVM1 execution.
//! (Test-only: requires crush-vm/crush-frontend/crush-lang-sdk as dev-deps)
#![cfg(test)]

/// Python source → CVM1 execution → output string.
pub fn run_python(source: &str) -> anyhow::Result<String> {
    let cast = crate::python_to_cast(source)
        .map_err(|e| anyhow::anyhow!("Python→CAST: {e}"))?;

    let mut compiler = crush_frontend::compiler::Compiler::new();
    let casm = compiler
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;

    let vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm)
        .map_err(|e| anyhow::anyhow!("CASM→CVM1: {e}"))?;

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
        "__crush_assign__", "__crush_deref__", "__crush_addr_of__",
        "__crush_not__", "__crush_neg__", "__crush_pos__",
        "__crush_subscript__", "__crush_unary__",
        "__crush_slice__", "__crush_contains__", "__crush_is__",
        "__crush_ifexpr__",
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
    fn test_simple() {
        assert_eq!(run_python("print(2 + 3 * 4)").unwrap(), "14");
    }

    #[test]
    fn test_function() {
        assert_eq!(
            run_python("def add(a,b):\n    return a + b\nprint(add(10,20))").unwrap(),
            "30"
        );
    }

    #[test]
    fn test_if_else() {
        assert_eq!(
            run_python("x = 5\nif x > 3:\n    print(100)\nelse:\n    print(0)").unwrap(),
            "100"
        );
    }

    #[test]
    fn test_while_loop() {
        assert_eq!(run_python("i = 0\nwhile i < 5:\n    i = i + 1\nprint(i)").unwrap(), "5");
    }

    #[test]
    fn test_for_loop() {
        assert_eq!(
            run_python("sum = 0\nfor i in range(0, 10):\n    sum = sum + i\nprint(sum)").unwrap(),
            "45"
        );
    }

    #[test]
    fn test_list_ops() {
        assert_eq!(
            run_python("arr = []\narr.append(10)\narr.append(20)\nprint(arr[0] + arr[1])").unwrap(),
            "30"
        );
    }

    #[test]
    fn test_slice_does_not_crash() {
        let r = run_python("arr = [1,2,3]\nprint(arr[0:2])");
        assert!(r.is_ok(), "slice should not crash: {:?}", r);
    }

    #[test]
    fn test_in_operator_does_not_crash() {
        let r = run_python("arr = [1,2,3]\nprint(2 in arr)");
        assert!(r.is_ok(), "'in' should not crash: {:?}", r);
    }

    // ── CRUSHAST-PYLOWER-1: comprehensions ──────────────────────────────────
    //
    // Real end-to-end runs: Python source → CAST → CASM → CVM1, asserting on
    // actual VM `output`, not just "lowering didn't panic" — per this repo's
    // own `ee75f1b` precedent (top-level statements that looked lowered but
    // were silently discarded and never executed).

    #[test]
    fn test_list_comprehension_assignment_runs_the_loop() {
        assert_eq!(
            run_python(concat!(
                "squares = [i * i for i in range(5)]\n",
                "total = 0\n",
                "for s in squares:\n",
                "    total = total + s\n",
                "print(total)\n",
            ))
            .unwrap(),
            "30" // 0 + 1 + 4 + 9 + 16
        );
    }

    #[test]
    fn test_list_comprehension_with_filter_as_call_argument() {
        assert_eq!(
            run_python("print(len([x for x in range(10) if x % 2 == 0]))").unwrap(),
            "5" // 0, 2, 4, 6, 8
        );
    }
}
