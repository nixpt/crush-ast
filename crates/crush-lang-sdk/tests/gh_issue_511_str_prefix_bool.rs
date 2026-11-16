//! #511: `str.starts_with` / `str.ends_with` must return Bool (`true` /
//! `false`) on every engine, like `str.contains`. They used to return Int
//! 1/0 on the capability path, so `str.starts_with("ab", "a") == true` was
//! silently false.
//!
//! The same program runs on the scheduler VM, PortableVm and FastVM; each
//! must give the same observable result.

use crush_lang_sdk::HostCapsBuilder;
use crush_vm::fastvm::FastYield;
use crush_vm::portable_vm::PortableVm;
use crush_vm::vm::Quotas;
use crush_vm::vm::Value;

fn compile(src: &str) -> (casm::Program, crush_vm::Program) {
    let casm = crush_lang_sdk::compile::compile_crush_to_casm(src).expect("compile");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");
    (casm, vm)
}

/// The issue's probes: (expression, expected). Each is returned from `main`
/// so the result is observable without depending on print formatting.
const PROBES: &[(&str, bool)] = &[
    (r#"str.starts_with("ab", "a")"#, true),
    (r#"str.ends_with("ab", "b")"#, true),
    (r#"str.contains("ab", "b")"#, true),
    (r#"str.starts_with("ab", "z")"#, false),
    (r#"str.ends_with("ab", "z")"#, false),
    (r#"str.starts_with("ab", "a") == true"#, true),
    (r#"str.ends_with("ab", "b") == true"#, true),
    (r#"str.starts_with("ab", "z") == false"#, true),
    (r#"str.starts_with("ab", "a") == 1"#, false),
];

fn src(expr: &str) -> String {
    format!("fn main() {{\n    return {expr};\n}}\n")
}

#[test]
fn scheduler_returns_bool() {
    for (expr, want) in PROBES {
        let (_, vm) = compile(&src(expr));
        let caps = HostCapsBuilder::new().stdlib(true).build();
        let r = crush_vm::run_with_caps(&vm, &Quotas::default(), Some(&caps)).expect("scheduler");
        assert_eq!(
            r.return_value,
            Some(Value::Bool(*want)),
            "scheduler: {expr}"
        );
    }
}

#[test]
fn portable_vm_returns_bool() {
    for (expr, want) in PROBES {
        let (_, vm) = compile(&src(expr));
        let mut pvm = PortableVm::new(vm);
        pvm.set_host_caps(HostCapsBuilder::new().stdlib(true).build());
        let r = pvm.run().expect("portable");
        assert_eq!(
            r.stack.last(),
            Some(&Value::Bool(*want)),
            "portable: {expr}"
        );
    }
}

/// FastVM has no host-capability bridge for the stdlib `str.*` names (run
/// from source it stops at `InvalidCapability`, for `str.contains` too), so
/// it is exercised through its native `StrStartsWith` / `StrEndsWith` /
/// `StrContains` ops: the compiled `cap_call` is rewritten to the CASM op the
/// frontend's direct-call path emits.
#[test]
fn fastvm_returns_bool() {
    for (expr, want) in PROBES {
        let (mut casm, _) = compile(&src(expr));
        for f in casm.functions.values_mut() {
            for i in &mut f.body {
                if i.op == "cap_call" {
                    let op = match i.args["name"].as_str() {
                        Some("str.starts_with") => "str_starts_with",
                        Some("str.ends_with") => "str_ends_with",
                        Some("str.contains") => "str_contains",
                        _ => continue,
                    };
                    i.op = op.to_string();
                    i.args = serde_json::json!({});
                }
            }
        }
        match crush_vm::run_fastvm(&casm) {
            Ok(FastYield::Finished(Some(v))) => {
                assert_eq!(format!("{v:?}"), format!("Bool({want})"), "fastvm: {expr}");
            }
            other => panic!("fastvm: {expr}: expected Finished(Some(..)), got {other:?}"),
        }
    }
}

/// Printing shows `true` / `false`, not `1` / `0`, on scheduler and PortableVm.
#[test]
fn print_shows_true_false() {
    let src = r#"
fn main() {
    print(str.starts_with("ab", "a"));
    print(str.ends_with("ab", "b"));
    print(str.starts_with("ab", "z"));
    print(str.starts_with("ab", "a") == true);
    return 0;
}
"#;
    let (_, vm) = compile(src);
    let caps = HostCapsBuilder::new().stdlib(true).build();
    let sched = crush_vm::run_with_caps(&vm, &Quotas::default(), Some(&caps)).expect("scheduler");
    let mut pvm = PortableVm::new(vm);
    pvm.set_host_caps(HostCapsBuilder::new().stdlib(true).build());
    let port = pvm.run().expect("portable");
    assert_eq!(sched.output, "true\ntrue\nfalse\ntrue\n");
    assert_eq!(port.output, sched.output);
}
