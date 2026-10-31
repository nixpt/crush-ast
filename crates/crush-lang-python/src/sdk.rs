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

    // CRUSH-196: slicing used to run against a stub that returned None; it is
    // now `seq.slice` (CPython slice semantics). `in` is still unimplemented
    // and must fail loudly.
    #[test]
    fn test_slice() {
        assert_eq!(run_python("arr = [1,2,3]\nprint(arr[0:2])").unwrap(), "[1, 2]");
    }

    #[test]
    fn test_in_operator_fails_loudly_until_implemented() {
        let err = run_python("arr = [1,2,3]\nprint(2 in arr)").unwrap_err();
        assert!(err.to_string().contains("__crush_contains__"), "{err}");
    }

    // ── CRUSHAST-PYLOWER-1: try/except, comprehensions ──────────────────────
    //
    // Real end-to-end runs: Python source → CAST → CASM → CVM1, asserting on
    // actual VM `output`, not just "lowering didn't panic" — per this repo's
    // own `ee75f1b` precedent (top-level statements that looked lowered but
    // were silently discarded and never executed).

    #[test]
    fn test_try_except_catches_and_reads_exception_field() {
        assert_eq!(
            run_python(
                "try:\n    raise ValueError(\"bad thing\")\nexcept ValueError as e:\n    print(e.message)\n"
            )
            .unwrap(),
            "bad thing"
        );
    }

    #[test]
    fn test_try_except_finally_always_runs() {
        assert_eq!(
            run_python(concat!(
                "x = 0\n",
                "try:\n",
                "    x = 1\n",
                "    raise ValueError(\"boom\")\n",
                "except ValueError:\n",
                "    x = 2\n",
                "finally:\n",
                "    x = x + 100\n",
                "print(x)\n",
            ))
            .unwrap(),
            "102"
        );
    }

    #[test]
    fn test_try_except_multi_handler_dispatches_by_type() {
        assert_eq!(
            run_python(concat!(
                "n = 5\n",
                "try:\n",
                "    if n < 0:\n",
                "        raise ValueError(\"negative\")\n",
                "    else:\n",
                "        raise TypeError(\"not a value error\")\n",
                "except ValueError:\n",
                "    print(1)\n",
                "except TypeError:\n",
                "    print(2)\n",
            ))
            .unwrap(),
            "2"
        );
    }

    #[test]
    fn test_try_no_exception_skips_handler() {
        assert_eq!(
            run_python(concat!(
                "x = 0\n",
                "try:\n",
                "    x = 1\n",
                "except ValueError:\n",
                "    x = 999\n",
                "print(x)\n",
            ))
            .unwrap(),
            "1"
        );
    }

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

    // ── CRUSHAST-PYLOWER-1: match ────────────────────────────────────────

    #[test]
    fn test_match_literal_and_wildcard_arms() {
        assert_eq!(
            run_python(concat!(
                "def code(n):\n",
                "    match n:\n",
                "        case 0:\n",
                "            return 100\n",
                "        case 1:\n",
                "            return 200\n",
                "        case _:\n",
                "            return 999\n",
                "print(code(0) + code(1) + code(5))\n",
            ))
            .unwrap(),
            "1299" // 100 + 200 + 999
        );
    }

    #[test]
    fn test_match_capture_pattern_binds_whole_subject() {
        assert_eq!(
            run_python("match 42:\n    case n:\n        print(n)\n").unwrap(),
            "42"
        );
    }

    // ── CRUSH-200: Python semantics vs Crush defaults ─────────────────────

    #[test]
    fn crush200_true_div_floor_div_mod() {
        assert_eq!(run_python("print(7 / 2)").unwrap(), "3.5");
        assert_eq!(run_python("print(-7 // 2)").unwrap(), "-4");
        assert_eq!(run_python("print(-7 % 2)").unwrap(), "1");
    }

    #[test]
    fn crush200_range_with_step() {
        assert_eq!(
            run_python(
                "out = []\nfor j in range(1, 10, 3):\n    out.append(j)\nprint(out[0], out[1], out[2])\n"
            )
            .unwrap(),
            "1 4 7"
        );
    }

    #[test]
    fn crush200_dict_set_and_iterate() {
        assert_eq!(
            run_python(
                "d = {\"a\": 1, \"b\": 2}\nd[\"c\"] = 3\nprint(d[\"c\"])\nkeys = []\nfor k in d:\n    keys.append(k)\nprint(keys[0], keys[1], keys[2])\n"
            )
            .unwrap(),
            "3\na b c"
        );
    }

    #[test]
    fn crush200_dict_missing_key_and_int_key_fail_loud() {
        let err = run_python("d = {\"a\": 1}\nprint(d[\"missing\"])\n").unwrap_err();
        assert!(
            err.to_string().contains("absent key") || err.to_string().contains("present map key"),
            "expected missing-key error, got: {err}"
        );
        let err = run_python("d = {}\nd[1] = 2\n").unwrap_err();
        assert!(
            err.to_string().contains("string key"),
            "expected non-string key rejection, got: {err}"
        );
    }

    #[test]
    fn crush200_print_sep_and_bool_none() {
        assert_eq!(run_python("print(1, 2, \"a\")").unwrap(), "1 2 a");
        assert_eq!(run_python("print(True, None, False)").unwrap(), "True None False");
    }

    #[test]
    fn crush200_decorator_and_for_else_fail_loud() {
        let err = run_python("@twice\ndef g():\n    return 21\nprint(g())\n").unwrap_err();
        assert!(
            err.to_string().contains("decorator"),
            "expected decorator bail, got: {err}"
        );
        let err = run_python("for i in range(1):\n    pass\nelse:\n    print(1)\n").unwrap_err();
        assert!(
            err.to_string().contains("for ... else"),
            "expected for-else bail, got: {err}"
        );
    }

    // CRUSH-zorro-38: keyword arguments were dropped (`print('a', end='')`
    // printed a newline). Unsupported ones are now walk-time errors.
    #[test]
    fn unsupported_keyword_arguments_fail_loud() {
        for (src, want) in [
            ("def f(a, b):\n    return a - b\nprint(f(b=1, a=10))\n", "keyword argument `b=` to `f`"),
            ("import sys\nprint('x', file=sys.stderr)\n", "keyword argument `file=` to `print`"),
            ("print('a,b'.split(sep=','))\n", "keyword argument `sep=` to `.split()`"),
            ("d = {}\nprint(1, **d)\n", "`**` keyword arguments"),
        ] {
            let err = run_python(src).unwrap_err();
            assert!(err.to_string().contains(want), "{src:?}: {err}");
        }
    }

    #[test]
    fn crush200_default_args_fail_loud() {
        let err = run_python("def g(n, greeting=\"Hello\"):\n    print(greeting, n)\ng(\"Bob\")\n")
            .unwrap_err();
        assert!(
            err.to_string().contains("default argument"),
            "expected default-arg bail, got: {err}"
        );
    }
}
