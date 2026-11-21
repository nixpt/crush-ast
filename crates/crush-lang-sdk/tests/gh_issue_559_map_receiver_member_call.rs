//! GitHub issue #559: a map receiver member call (`m.arr_get`, `m.arr_set`,
//! `m.push`) used to resolve differently depending on whether an unrelated
//! struct in the program declared a method of the same name.
//!
//! Per `docs/design/object-model.md` §9.4, a map receiver does field lookup
//! only — on both the scheduler VM and PortableVm — and other engines
//! (FastVM, JIT, AOT) refuse loudly as they do for member calls today.

use crush_vm::fastvm::FastYield;
use crush_vm::{PortableVm, Quotas};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Ok(String),
    Err(String),
}

fn compile(source: &str) -> (casm::Program, crush_vm::Program) {
    let casm = crush_frontend::compile_crush_source(source)
        .unwrap_or_else(|e| panic!("compile failed: {e:#}\n{source}"));
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm)
        .unwrap_or_else(|e| panic!("casm_to_vm failed: {e:#}\n{source}"));
    (casm, prog)
}

fn run_both_vms(source: &str) -> (Outcome, Outcome) {
    run_both_vms_with_quotas(source, &Quotas::default())
}

fn run_both_vms_with_quotas(source: &str, quotas: &Quotas) -> (Outcome, Outcome) {
    let (_casm, prog) = compile(source);
    let sched = match crush_vm::run(&prog, quotas) {
        Ok(r) => Outcome::Ok(r.output),
        Err(e) => Outcome::Err(e.to_string()),
    };
    let mut pvm = PortableVm::new(prog);
    pvm.set_quotas(quotas.clone());
    let portable = match pvm.run() {
        Ok(r) => Outcome::Ok(r.output),
        Err(e) => Outcome::Err(e.to_string()),
    };
    (sched, portable)
}

fn assert_fastvm_refuses(source: &str) {
    let (casm, _prog) = compile(source);
    let casm_err = match crush_vm::run_fastvm(&casm) {
        Err(e) => e.to_string(),
        Ok(FastYield::Error(e)) => e.to_string(),
        Ok(other) => {
            panic!("FastVM should refuse map member call, got {other:?}\n{source}")
        }
    };
    assert!(
        casm_err.contains("call_member")
            || casm_err.contains("Unknown opcode")
            || casm_err.contains("COSM")
            || casm_err.contains("make_closure")
            || casm_err.contains("non-array"),
        "expected FastVM refusal, got: {casm_err}\n{source}"
    );
}

/// Repro 1 from #559: `m.arr_get("k")` on `{"k": 1}` must do map field lookup
/// only (failing with `map has no member \`arr_get\``), with and without an
/// unrelated struct declaring `arr_get`.
#[test]
fn map_arr_get_resolves_identically_with_and_without_unrelated_struct() {
    let without_struct = r#"
let m = {"k": 1}
print(m.arr_get("k"))
"#;
    let with_struct = r#"
struct Unrelated { x }
impl Unrelated { fn arr_get(self, k) { return 999 } }
let m = {"k": 1}
print(m.arr_get("k"))
"#;

    let (s_no, p_no) = run_both_vms(without_struct);
    let (s_yes, p_yes) = run_both_vms(with_struct);

    assert_eq!(s_no, p_no, "scheduler vs PortableVm without struct");
    assert_eq!(s_yes, p_yes, "scheduler vs PortableVm with struct");
    assert_eq!(s_no, s_yes, "without struct vs with struct must match");
    match &s_no {
        Outcome::Err(msg) => assert!(
            msg.contains("map has no member `arr_get`"),
            "unexpected error: {msg}"
        ),
        Outcome::Ok(out) => panic!("expected map member error, got output: {out:?}"),
    }

    assert_fastvm_refuses(without_struct);
    assert_fastvm_refuses(with_struct);
}

/// Companion to repro 1: `m.arr_set("k", 2)` on `{"k": 1}` also does map field
/// lookup only, with and without an unrelated struct declaring `arr_set`.
#[test]
fn map_arr_set_resolves_identically_with_and_without_unrelated_struct() {
    let without_struct = r#"
let m = {"k": 1}
m.arr_set("k", 2)
"#;
    let with_struct = r#"
struct Unrelated { x }
impl Unrelated { fn arr_set(self, k, v) { return 0 } }
let m = {"k": 1}
m.arr_set("k", 2)
"#;

    let (s_no, p_no) = run_both_vms(without_struct);
    let (s_yes, p_yes) = run_both_vms(with_struct);

    assert_eq!(s_no, p_no, "scheduler vs PortableVm without struct");
    assert_eq!(s_yes, p_yes, "scheduler vs PortableVm with struct");
    assert_eq!(s_no, s_yes, "without struct vs with struct must match");
    match &s_no {
        Outcome::Err(msg) => assert!(
            msg.contains("map has no member `arr_set`"),
            "unexpected error: {msg}"
        ),
        Outcome::Ok(out) => panic!("expected map member error, got output: {out:?}"),
    }

    assert_fastvm_refuses(without_struct);
    assert_fastvm_refuses(with_struct);
}

/// Repro 2 from #559: `m.push(4)` on `{"push": |v| => v * 2}` must call the
/// stored closure and print `8`, with and without an unrelated struct
/// declaring `push`.
#[test]
fn map_push_calls_stored_function_with_and_without_unrelated_struct() {
    let without_struct = r#"
let m = {"push": |v| => v * 2}
print(m.push(4))
"#;
    let with_struct = r#"
struct Unrelated { x }
impl Unrelated { fn push(self, v) { return v + 100 } }
let m = {"push": |v| => v * 2}
print(m.push(4))
"#;

    let (s_no, p_no) = run_both_vms(without_struct);
    let (s_yes, p_yes) = run_both_vms(with_struct);

    assert_eq!(s_no, Outcome::Ok("8\n".into()));
    assert_eq!(p_no, Outcome::Ok("8\n".into()));
    assert_eq!(s_yes, Outcome::Ok("8\n".into()));
    assert_eq!(p_yes, Outcome::Ok("8\n".into()));

    assert_fastvm_refuses(without_struct);
    assert_fastvm_refuses(with_struct);
}

/// Same invariants when the map arrives through an unannotated parameter or
/// a variable reassigned from an array literal (so the compiler cannot prove
/// the receiver's type).
#[test]
fn map_via_param_and_reassignment_resolves_identically() {
    let push_via_param_no = r#"
fn call_push(x) { return x.push(4) }
fn main() {
    let m = {"push": |v| => v * 2}
    print(call_push(m))
    let xs = [1, 2]
    print(call_push(xs))
}
"#;
    let push_via_param_yes = r#"
struct Unrelated { x }
impl Unrelated { fn push(self, v) { return v + 100 } }
fn call_push(x) { return x.push(4) }
fn main() {
    let m = {"push": |v| => v * 2}
    print(call_push(m))
    let xs = [1, 2]
    print(call_push(xs))
}
"#;
    let (s_no, p_no) = run_both_vms(push_via_param_no);
    let (s_yes, p_yes) = run_both_vms(push_via_param_yes);
    assert_eq!(s_no, Outcome::Ok("8\n[1, 2, 4]\n".into()));
    assert_eq!(p_no, s_no);
    assert_eq!(s_yes, s_no);
    assert_eq!(p_yes, s_no);

    let arr_get_via_param_no = r#"
fn call_get(x, k) { return x.arr_get(k) }
fn main() {
    let m = {"k": 1}
    print(call_get(m, "k"))
}
"#;
    let arr_get_via_param_yes = r#"
struct Unrelated { x }
impl Unrelated { fn arr_get(self, k) { return 0 } }
fn call_get(x, k) { return x.arr_get(k) }
fn main() {
    let m = {"k": 1}
    print(call_get(m, "k"))
}
"#;
    let (s_no, p_no) = run_both_vms(arr_get_via_param_no);
    let (s_yes, p_yes) = run_both_vms(arr_get_via_param_yes);
    assert_eq!(s_no, p_no);
    assert_eq!(s_yes, p_yes);
    assert_eq!(s_no, s_yes);
    match &s_no {
        Outcome::Err(msg) => assert!(msg.contains("map has no member `arr_get`"), "{msg}"),
        Outcome::Ok(out) => panic!("expected error, got {out:?}"),
    }

    // Reassigned from an array literal to a map: must NOT stay on the
    // provably-array fast path.
    let reassigned = r#"
fn main() {
    let x = [1]
    x = {"push": |v| => v * 3}
    print(x.push(5))
}
"#;
    let (s, p) = run_both_vms(reassigned);
    assert_eq!(s, Outcome::Ok("15\n".into()));
    assert_eq!(p, s);
}

/// A map storing a function under `"arr_get"` calls it; function-call form
/// `arr_get(m, "k")` and subscript `m["k"]` still index the map by key.
#[test]
fn map_field_named_arr_get_and_function_form_coexist() {
    let src = r#"
fn main() {
    let m = {"arr_get": |k| => k + "!", "k": 7}
    print(m.arr_get("ok"))
    print(arr_get(m, "k"))
    print(m["k"])
}
"#;
    let (s, p) = run_both_vms(src);
    assert_eq!(s, Outcome::Ok("ok!\n7\n7\n".into()));
    assert_eq!(p, s);
}

/// Struct method, map field function, and array built-in all named `push`
/// coexist in the same program on both VMs.
#[test]
fn struct_map_and_array_push_coexist() {
    let src = r#"
struct Box { base }
impl Box { fn push(self, v) { return self.base + v } }
fn main() {
    let b = Box { base: 10 }
    let m = {"push": |v| => v * 2}
    let xs = [1, 2]
    xs.push(3)
    print(b.push(5))
    print(m.push(4))
    print(xs)
}
"#;
    let (s, p) = run_both_vms(src);
    assert_eq!(s, Outcome::Ok("15\n8\n[1, 2, 3]\n".into()));
    assert_eq!(p, s);
}

/// Pins §9.6's documented `Quotas::allowed_caps` contract:
/// - `CALL_MEMBER`'s §9.4 fallback (`fn f(xs) { xs.push(2) }`) and the
///   `array.push` opcode are VM-internal and not subject to `allowed_caps`.
/// - Provably-array `xs.push(2)` lowered via §9.1 rule 5 to `CAP_CALL "push"`
///   passes through `dispatch_cap` and is subject to `allowed_caps`.
#[test]
fn allowed_caps_matches_section_9_6_contract() {
    let quotas = Quotas {
        allowed_caps: Some(vec!["io.print".into()]),
        ..Quotas::default()
    };

    // CALL_MEMBER fallback on an array parameter: not gated by allowed_caps.
    let member_route = r#"
fn grow(xs) { xs.push(2); return xs }
fn main() { print(grow([1])) }
"#;
    let (s, p) = run_both_vms_with_quotas(member_route, &quotas);
    assert_eq!(s, Outcome::Ok("[1, 2]\n".into()));
    assert_eq!(p, s);

    // Opcode form `array.push`: not gated by allowed_caps.
    let op_route = r#"
fn main() { let xs = [1]; array.push(xs, 2); print(xs) }
"#;
    let (s, p) = run_both_vms_with_quotas(op_route, &quotas);
    assert_eq!(s, Outcome::Ok("[1, 2]\n".into()));
    assert_eq!(p, s);

    // Provably-primitive rule-5 cap_call route: gated by allowed_caps.
    let cap_route = r#"
fn main() { let xs = [1]; xs.push(2); print(xs) }
"#;
    let (s, p) = run_both_vms_with_quotas(cap_route, &quotas);
    assert_eq!(s, p);
    match &s {
        Outcome::Err(msg) => assert!(msg.contains("push"), "expected CapDenied for push: {msg}"),
        Outcome::Ok(out) => panic!("expected allowed_caps denial on cap_call route, got {out:?}"),
    }
}

/// Regression check from #559 review 1: an array passed as a function
/// parameter (`xs.push(3); return xs.pop()`) lowers uniformly to `call_member`
/// and runs on the scheduler VM, PortableVm, and FastVM (both CASM and
/// bytecode paths).
#[test]
fn param_array_push_and_pop_runs_on_vms_and_fastvm() {
    let source = r#"
fn add(xs) {
    xs.push(3)
    return xs.pop()
}
fn main() { return add([1, 2]) }
"#;
    let (casm, prog) = compile(source);

    let sched = crush_vm::run(&prog, &Quotas::default()).expect("scheduler");
    assert_eq!(sched.return_value, Some(crush_vm::vm::Value::Int(3)));

    let portable = PortableVm::new(prog.clone()).run().expect("PortableVm");
    assert_eq!(
        portable.return_value.or(portable.stack.last().cloned()),
        Some(crush_vm::vm::Value::Int(3))
    );

    let fast_casm = crush_vm::run_fastvm(&casm).expect("FastVM CASM");
    assert_eq!(
        fast_casm,
        FastYield::Finished(Some(crush_vm::RuntimeValue::Int(3)))
    );
}

/// Pins #559 behaviour change (c): `xs.parallel_map(f)` in receiver form on a
/// parameter lowers to `call_member` and refuses loudly on both VM engines
/// with the `parallel_map(xs, f)` hint instead of falling back to `cap_call`.
#[test]
fn param_array_parallel_map_member_call_refuses_loudly() {
    let source = r#"
fn run(xs, f) { return xs.parallel_map(f) }
fn main() { print(run([1, 2, 3], |x| => x * 2)) }
"#;
    let (sched, portable) = run_both_vms(source);
    assert_eq!(sched, portable, "scheduler vs PortableVm");
    match &sched {
        Outcome::Err(msg) => {
            assert!(
                msg.contains(
                    "`.parallel_map(…)` on an array through a member call is not supported"
                ),
                "expected member-call refusal, got: {msg}"
            );
            assert!(
                msg.contains("parallel_map(xs, f)"),
                "expected `parallel_map(xs, f)` hint, got: {msg}"
            );
        }
        Outcome::Ok(out) => {
            panic!("expected parallel_map member-call refusal, got output: {out:?}")
        }
    }
    assert_fastvm_refuses(source);
}
