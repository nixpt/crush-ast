//! GitHub issue #378 (sibling of CRUSH-234's AI fix): the ten `DOM_*`
//! opcodes fail closed. An ungranted opcode is a named `VmError::CapDenied`
//! — never a silent `Value::Null` — and a granted handler's error
//! propagates instead of being flattened into null.
//!
//! DOM opcodes are not reachable from `.crush` text today (the frontend
//! lowers `dom_query` to NOP; CRUSH-252's real-provider work owns wiring
//! them), so the programs here are built at the CVM1 bytecode level: a
//! single `DOM_QUERY <const>` + `HALT`.

use std::process::{Command, Stdio};

use crush_vm::bytecode::{DOM_QUERY, HALT, Program};
use crush_vm::host::{HostCap, HostCapSpec};
use crush_vm::portable_vm::PortableVm;
use crush_vm::vm::{Quotas, Value};
use crush_vm::{HostCaps, run_with_caps};

const WANT: &str = "dom_native.query: DOM op 'query' not granted / no DOM host registered";

/// `DOM_QUERY` with the string operand (const-pool index 0) and `HALT`.
fn dom_program() -> Program {
    Program {
        consts: vec!["#app".to_string()],
        code: vec![DOM_QUERY, 0, 0, HALT],
        ..Default::default()
    }
}

/// A granted handler (`Value` isn't `Send`, so the returned value is built
/// in `call`).
struct Stub;

impl HostCap for Stub {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "dom_native.query".into(),
            argc: Some(0),
            returns: true,
        }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Ok(Some(Value::Str("node-1".into())))
    }
}

/// A granted handler that fails, like a real DOM backend can.
struct Boom;

impl HostCap for Boom {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "dom_native.query".into(),
            argc: Some(0),
            returns: true,
        }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Err("boom".into())
    }
}

/// GitHub issue #378, part 1: ungranted is a named `CapDenied` on the
/// scheduler — the same text with no host caps at all.
#[test]
fn ungranted_dom_opcode_is_a_named_error_on_the_scheduler() {
    let program = dom_program();
    let err = run_with_caps(&program, &Quotas::default(), Some(&HostCaps::new()))
        .expect_err("ungranted must refuse");
    let msg = err.to_string();
    assert!(msg.contains(WANT), "{msg}");
    let bare = run_with_caps(&program, &Quotas::default(), None).expect_err("no caps must refuse");
    assert_eq!(bare.to_string(), msg, "same message with no caps at all");
}

/// The PortableVm twin raises the identical named error.
#[test]
fn ungranted_dom_opcode_is_the_same_named_error_on_the_portable_vm() {
    let program = dom_program();
    let mut pvm = PortableVm::new(program.clone());
    pvm.set_host_caps(HostCaps::new());
    let portable = pvm.run().expect_err("PortableVm must refuse").to_string();
    let sched = run_with_caps(&program, &Quotas::default(), Some(&HostCaps::new()))
        .expect_err("scheduler must refuse")
        .to_string();
    assert!(portable.contains(WANT), "{portable}");
    assert_eq!(portable, sched, "engines agree on the message");
}

/// GitHub issue #378, part 2: a granted handler's error propagates — on
/// both engines — instead of being swallowed into a null push.
#[test]
fn a_failing_handler_propagates_on_the_scheduler_and_the_portable_vm() {
    let program = dom_program();
    let boom = || {
        let mut caps = HostCaps::new();
        caps.register(Box::new(Boom));
        caps
    };
    let sched = run_with_caps(&program, &Quotas::default(), Some(&boom()))
        .expect_err("a failing handler must propagate");
    let msg = sched.to_string();
    assert!(
        msg.contains("dom_native.query") && msg.contains("boom"),
        "{msg}"
    );
    let mut pvm = PortableVm::new(program);
    pvm.set_host_caps(boom());
    let portable = pvm
        .run()
        .expect_err("PortableVm must propagate")
        .to_string();
    assert_eq!(portable, msg, "engines agree on the propagated error");
}

/// The granted success path is unchanged: a returning handler's value is
/// pushed, on both engines.
#[test]
fn a_granted_handler_still_pushes_its_value() {
    let program = dom_program();
    let caps = || {
        let mut caps = HostCaps::new();
        caps.register(Box::new(Stub));
        caps
    };
    let sched =
        run_with_caps(&program, &Quotas::default(), Some(&caps())).expect("granted must run");
    assert_eq!(sched.stack.last(), Some(&Value::Str("node-1".into())));
    let mut pvm = PortableVm::new(program);
    pvm.set_host_caps(caps());
    let portable = pvm.run().expect("granted must run");
    assert_eq!(portable.stack.last(), Some(&Value::Str("node-1".into())));
}

/// The embedder's allowlist (`Quotas::allowed_caps`) covers DOM opcodes as it
/// covers CAP_CALL: a granted op the allowlist leaves out is refused, on both
/// engines, and its handler never runs.
#[test]
fn the_embedder_allowlist_refuses_a_granted_dom_op() {
    let program = dom_program();
    let caps = || {
        let mut caps = HostCaps::new();
        caps.register(Box::new(Stub));
        caps
    };
    let quotas = Quotas {
        allowed_caps: Some(vec!["io.print".into()]),
        ..Default::default()
    };
    let sched = run_with_caps(&program, &quotas, Some(&caps()))
        .expect_err("an op outside the allowlist must refuse")
        .to_string();
    assert!(sched.contains("dom_native.query"), "{sched}");
    let mut pvm = PortableVm::new(program);
    pvm.set_host_caps(caps());
    pvm.set_quotas(quotas);
    let portable = pvm.run().expect_err("PortableVm must refuse").to_string();
    assert_eq!(portable, sched, "engines agree on the refusal");
}

/// FastVM's embedder seam calls the granted handler (it used to answer every
/// granted DOM request with a fixed `{ok:true,...}` string without calling
/// it): a value comes back, a failing handler resolves to nothing.
#[test]
fn fastvm_resolve_host_request_calls_the_dom_handler() {
    use crush_vm::fastvm::{HostRequest, resolve_host_request};
    use crush_vm::value::RuntimeValue;
    let req = HostRequest::DomQuery {
        args: serde_json::json!({}),
    };
    let mut stub = HostCaps::new();
    stub.register(Box::new(Stub));
    assert_eq!(
        resolve_host_request(&req, Some(&stub)),
        Some(RuntimeValue::String("node-1".into()))
    );
    let mut boom = HostCaps::new();
    boom.register(Box::new(Boom));
    assert_eq!(resolve_host_request(&req, Some(&boom)), None);
}

/// #423: the FastVM seam applies the embedder's allowlist as the scheduler and
/// PortableVm do. Under an allowlist that leaves `dom_native.query` out, a
/// granted handler is not called (`None`); inside it, it is.
#[test]
fn fastvm_resolve_host_request_applies_the_embedder_allowlist() {
    use crush_vm::fastvm::{HostRequest, resolve_host_request_with_quotas};
    use crush_vm::value::RuntimeValue;
    let req = HostRequest::DomQuery {
        args: serde_json::json!({}),
    };
    let mut stub = HostCaps::new();
    stub.register(Box::new(Stub));
    let refused = Quotas {
        allowed_caps: Some(vec!["io.print".to_string()]),
        ..Quotas::default()
    };
    assert_eq!(
        resolve_host_request_with_quotas(&req, Some(&stub), &refused),
        None,
        "an op outside the allowlist must not reach the handler"
    );
    let allowed = Quotas {
        allowed_caps: Some(vec!["dom_native.*".to_string()]),
        ..Quotas::default()
    };
    assert_eq!(
        resolve_host_request_with_quotas(&req, Some(&stub), &allowed),
        Some(RuntimeValue::String("node-1".into()))
    );
}

/// End-to-end through the standalone runner: a `.cvm1` whose only effect is
/// a `DOM_QUERY` dies with the named refusal on stderr (`crush-vm run` has
/// no host grant source, so its registry is empty).
#[test]
fn crush_vm_run_names_the_refusal_on_stderr() {
    let blob = dom_program().to_blob();
    let path = std::env::temp_dir().join(format!(
        "crush-vm-dom-fail-closed-{}.cvm1",
        std::process::id()
    ));
    std::fs::write(&path, &blob).expect("write fixture");
    let result = Command::new(env!("CARGO_BIN_EXE_crush-vm"))
        .arg("run")
        .arg(&path)
        .stdin(Stdio::null())
        .output()
        .expect("run crush-vm");
    let _ = std::fs::remove_file(&path);
    assert!(!result.status.success(), "must refuse to run");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains(WANT), "unexpected stderr: {stderr}");
}
