//! GitHub issue #435: a struct method named like a built-in receiver call
//! (`pop`, `push`, …) hijacked every `.pop()` / `.push()` in the program.
//!
//! The compiler routes `x.m(…)` with a program-wide set: if any struct
//! declares a method `m`, every `x.m(…)` compiled to `call_member`, which
//! fails at run time on an array ("member calls resolve on struct instances
//! (COSM Records) and maps"). A call on a computed receiver (`self.a.push(1)`,
//! `m.a.push(1)`, `xs[0].push(2)`) always compiled to `call_member`, struct
//! or no struct.
//!
//! The hijack covers exactly the names the compiler keeps as built-in
//! receiver calls (`RECEIVER_CAPABILITIES`: `push pop append arr_get arr_set
//! arr_slice parallel_map make_range`). Other member names (`len`, `get`,
//! `set`, `keys`, `contains`) are not built-in receiver calls: `xs.len()` is
//! a member call (and a runtime error on an array) with or without a struct
//! method of that name, so there is nothing to hijack; `other_member_names_*`
//! pins that.
//!
//! The fix is the runtime fallback of `docs/design/object-model.md` §9.4:
//! `CALL_MEMBER` on a receiver that is not a Record, a Kind or a map runs
//! the built-in receiver call of that name (one implementation,
//! `crush_vm::receiver_builtins`, shared with the capability dispatch). A
//! Record keeps its own method, and a Record without one keeps the
//! "has no method" error. `parallel_map` calls back into user code, so it is
//! not a fallback row: through `CALL_MEMBER` it is a loud error naming the
//! function form.
//!
//! `CALL_MEMBER` runs on the scheduler VM and PortableVm; each test runs both
//! and requires the same output. FastVM and the JIT refuse it at lowering
//! (design §9.4, "Backends"), which `fastvm_refuses_*` pins.

use crush_lang_sdk::{HostCapsBuilder, Runtime};
use crush_vm::PortableVm;

/// The scheduler VM's printed output, or its error text.
fn scheduler(source: &str) -> Result<String, String> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .unwrap_or_else(|e| panic!("compile: {e:#}\n{source}"));
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

/// PortableVm's printed output, or its error text.
fn portable(source: &str) -> Result<String, String> {
    let prog = crush_lang_sdk::compile::compile_crush_source(source)
        .unwrap_or_else(|e| panic!("compile: {e:#}\n{source}"));
    let mut vm = PortableVm::new(prog);
    vm.run().map(|r| r.output).map_err(|e| e.to_string())
}

/// Both VMs that run Kinds print `expected`.
fn assert_prints(source: &str, expected: &str) {
    assert_eq!(
        scheduler(source).as_deref(),
        Ok(expected),
        "scheduler VM\n{source}"
    );
    assert_eq!(
        portable(source).as_deref(),
        Ok(expected),
        "PortableVm\n{source}"
    );
}

/// The issue's first repro: `impl S { fn pop(self) … fn push(self, v) … }`
/// and `xs.push(4)` / `xs.pop()` on a plain array.
#[test]
fn a_struct_method_named_pop_or_push_does_not_hijack_array_calls() {
    assert_prints(
        "struct S { a }\nimpl S {\n    fn pop(self) { return 1 }\n    fn push(self, v) { return 2 }\n}\nfn main() {\n    let xs = [1, 2, 3]\n    xs.push(4)\n    print(xs.pop())\n    print(xs)\n}\n",
        "4\n[1, 2, 3]\n",
    );
}

/// The struct's own methods still resolve on its instances, in the same
/// program as the array calls.
#[test]
fn the_struct_methods_still_resolve_on_instances() {
    assert_prints(
        "struct S { a }\nimpl S {\n    fn pop(self) { return \"S.pop\" }\n    fn push(self, v) { return \"S.push \" + v }\n}\nfn main() {\n    let s = S { a: 0 }\n    let xs = [1]\n    print(s.pop())\n    print(s.push(7))\n    xs.push(2)\n    print(xs.pop())\n    print(s.pop())\n}\n",
        "S.pop\nS.push 7\n2\nS.pop\n",
    );
}

/// The owner's second repro (awesome-crush#6): no struct defines `push`, yet
/// `self.a.push(1)` on an array field compiled to `call_member`.
#[test]
fn push_on_an_array_field_of_self() {
    assert_prints(
        "struct S { a }\nimpl S { fn f(self) { self.a.push(1); return len(self.a) } }\nfn main() {\n    let xs = []\n    xs.push(5)\n    print(len(xs))\n    let s = S { a: [] }\n    print(s.f())\n}\n",
        "1\n1\n",
    );
}

/// A computed receiver when a struct *does* declare the method: the field is
/// an array, so the built-in call runs; a field holding an `S` resolves the
/// method.
#[test]
fn computed_receivers_with_a_conflicting_method() {
    assert_prints(
        "struct S { a }\nimpl S { fn push(self, v) { return \"S.push \" + v } }\nstruct H { items, s }\nfn main() {\n    let h = H { items: [], s: S { a: 0 } }\n    h.items.push(9)\n    print(h.items)\n    print(h.s.push(3))\n}\n",
        "[9]\nS.push 3\n",
    );
}

/// Every built-in receiver call, each next to a struct method of the same
/// name, gives what it gives with no such method.
#[test]
fn every_builtin_receiver_name_survives_a_struct_method_of_that_name() {
    // (method, call, expected print of the call, expected print of xs after)
    let cases: &[(&str, &str, &str)] = &[
        ("push", "xs.push(4)", "[1, 2, 3, 4]\n[1, 2, 3, 4]\n"),
        ("pop", "xs.pop()", "3\n[1, 2]\n"),
        ("append", "xs.append(4)", "[1, 2, 3, 4]\n[1, 2, 3, 4]\n"),
        ("arr_get", "xs.arr_get(0)", "1\n[1, 2, 3]\n"),
        ("arr_set", "xs.arr_set(0, 9)", "[9, 2, 3]\n[9, 2, 3]\n"),
        ("arr_slice", "xs.arr_slice(0, 1)", "[1]\n[1, 2, 3]\n"),
    ];
    for (method, call, expected) in cases {
        let plain =
            format!("fn main() {{\n    let xs = [1, 2, 3]\n    print({call})\n    print(xs)\n}}\n");
        let with_method = format!(
            "struct S {{ a }}\nimpl S {{ fn {method}(self) {{ return 0 }} }}\nfn main() {{\n    let xs = [1, 2, 3]\n    print({call})\n    print(xs)\n}}\n"
        );
        assert_eq!(
            scheduler(&plain).as_deref(),
            Ok(*expected),
            "plain {method}"
        );
        assert_prints(&with_method, expected);
    }
    // `make_range` is a receiver call on an Int.
    assert_prints(
        "struct S { a }\nimpl S { fn make_range(self) { return 0 } }\nfn main() {\n    let n = 0\n    print(n.make_range(3))\n}\n",
        "[0, 1, 2]\n",
    );
}

/// `parallel_map` calls back into user code, so it is not a fallback row:
/// next to a struct method of that name it is a loud error that names the
/// function form, on both VMs; with no such method it is unchanged.
#[test]
fn parallel_map_through_a_member_call_is_a_clear_error() {
    let plain = "fn main() {\n    let xs = [1, 2, 3]\n    print(xs.parallel_map(|n| n * 2))\n}\n";
    assert_eq!(scheduler(plain).as_deref(), Ok("[2, 4, 6]\n"));
    let with_method = "struct S { a }\nimpl S { fn parallel_map(self) { return 0 } }\nfn main() {\n    let xs = [1, 2, 3]\n    print(xs.parallel_map(|n| n * 2))\n}\n";
    for e in [
        scheduler(with_method).expect_err("errors"),
        portable(with_method).expect_err("errors"),
    ] {
        assert!(e.contains("write `parallel_map(xs, f)`"), "{e}");
    }
}

/// `len`, `get`, `set`, `keys`, `contains` are not built-in receiver calls:
/// on an array they are the same runtime error with or without a struct
/// method of the name, and on the struct they call the method.
#[test]
fn other_member_names_are_member_calls_with_or_without_a_struct_method() {
    for (method, call) in [
        ("len", "xs.len()"),
        ("get", "xs.get(0)"),
        ("set", "xs.set(0, 9)"),
        ("keys", "xs.keys()"),
        ("contains", "xs.contains(1)"),
    ] {
        let plain = format!("fn main() {{\n    let xs = [1, 2, 3]\n    print({call})\n}}\n");
        let with_method = format!(
            "struct S {{ a }}\nimpl S {{ fn {method}(self) {{ return 0 }} }}\nfn main() {{\n    let xs = [1, 2, 3]\n    print({call})\n}}\n"
        );
        let want = format!(
            "`.{method}(…)` on a array: member calls resolve on struct instances (COSM Records) and maps"
        );
        for src in [&plain, &with_method] {
            let e = scheduler(src).expect_err("errors");
            assert!(e.contains(&want), "{method}: {e}");
        }
        assert_prints(
            &format!(
                "struct S {{ a }}\nimpl S {{ fn {method}(self) {{ return \"S.{method}\" }} }}\nfn main() {{\n    let s = S {{ a: 0 }}\n    print(s.{method}())\n}}\n"
            ),
            &format!("S.{method}\n"),
        );
    }
}

/// A computed receiver in a program with no struct at all (a map field, an
/// indexed element).
#[test]
fn computed_receivers_without_structs() {
    assert_prints(
        "fn main() {\n    let m = {\"a\": []}\n    m.a.push(1)\n    let xs = [[1]]\n    xs[0].push(2)\n    print(m.a.pop())\n    print(xs)\n}\n",
        "1\n[[1, 2]]\n",
    );
}

/// FastVM (and the JIT built on it) refuses `CALL_MEMBER` when it lowers the
/// program: loud, never a silent result (design §9.4, "Backends").
#[test]
fn fastvm_refuses_a_computed_receiver_call_loudly() {
    let src = "fn main() {\n    let m = {\"a\": []}\n    m.a.push(1)\n    return len(m.a)\n}\n";
    let casm = crush_frontend::compile_crush_source(src).expect("compiles");
    let e = crush_vm::run_fastvm(&casm).expect_err("FastVM refuses");
    assert!(e.to_string().contains("call_member"), "{e}");
}

/// A struct instance that has no method of the name, in a program where
/// another struct has one, keeps the "has no method" error: a Record never
/// falls back to a built-in (design §9.2).
#[test]
fn an_instance_without_the_method_is_a_clear_error() {
    let e = scheduler(
        "struct S { a }\nimpl S { fn pop(self) { return 1 } }\nstruct T { b }\nfn main() {\n    let t = T { b: 0 }\n    print(t.pop())\n}\n",
    )
    .expect_err("errors");
    assert!(e.contains("`T` has no method `pop`"), "{e}");
}
