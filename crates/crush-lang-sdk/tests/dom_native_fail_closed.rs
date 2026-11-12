//! GitHub issue #378 (sibling of CRUSH-234's AI fix): the ten `dom_native.*`
//! opcodes fail closed. An ungranted opcode is a named `VmError::CapDenied`
//! — never a silent `Value::Null` — the same message on the scheduler, the
//! PortableVm and the FastVM embedder seam; a granted handler's error
//! propagates; the registered stubs still work.
//!
//! DOM opcodes are not reachable from `.crush` text today (the frontend
//! lowers `dom_query` to NOP; CRUSH-252's real-provider work owns wiring
//! them), so the programs here are built at the CVM1 bytecode level: a
//! single `DOM_QUERY <const>` + `HALT`.

use crush_lang_sdk::dom_native;
use crush_lang_sdk::{HostCap, HostCapSpec, HostCaps};
use crush_vm::bytecode::{DOM_QUERY, HALT, Program};
use crush_vm::fastvm::{HostRequest, dom_request_denied};
use crush_vm::portable_vm::PortableVm;
use crush_vm::run_with_caps;
use crush_vm::vm::{Quotas, Value};

const WANT: &str = "dom_native.query: DOM op 'query' not granted / no DOM host registered";

/// `DOM_QUERY` with the string operand (const-pool index 0) and `HALT`.
fn dom_program() -> Program {
    Program {
        consts: vec!["#app".to_string()],
        code: vec![DOM_QUERY, 0, 0, HALT],
        ..Default::default()
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

fn caps_with(register_stubs: bool) -> HostCaps {
    let mut caps = HostCaps::new();
    if register_stubs {
        dom_native::register(&mut caps);
    }
    caps
}

/// Ungranted: the named refusal, identical on the scheduler and the
/// PortableVm, with or without any host registry at all.
#[test]
fn ungranted_dom_ops_fail_closed_with_the_same_error_on_every_engine() {
    let program = dom_program();
    let sched = run_with_caps(&program, &Quotas::default(), Some(&caps_with(false)))
        .expect_err("scheduler must refuse")
        .to_string();
    assert!(sched.contains(WANT), "{sched}");
    let mut pvm = PortableVm::new(program.clone());
    pvm.set_host_caps(caps_with(false));
    let portable = pvm.run().expect_err("PortableVm must refuse").to_string();
    assert_eq!(portable, sched, "engines agree on the message");
    let bare = run_with_caps(&program, &Quotas::default(), None)
        .expect_err("no caps must refuse")
        .to_string();
    assert_eq!(bare, sched);

    // FastVM: the embedder seam names the same error for every DOM request
    // kind, granted or not.
    let req = HostRequest::DomQuery {
        args: serde_json::json!({}),
    };
    let denied = dom_request_denied(&req, Some(&caps_with(false))).expect("denied");
    assert!(
        denied
            .to_string()
            .contains("dom_native.query: DOM op 'query' not granted"),
        "{denied}"
    );
    assert!(dom_request_denied(&req, None).is_some());
    let granted = caps_with(true);
    assert!(dom_request_denied(&req, Some(&granted)).is_none());
    // Non-DOM requests are not its business.
    assert!(
        dom_request_denied(
            &HostRequest::AiQuery {
                args: serde_json::json!({})
            },
            None
        )
        .is_none()
    );
}

/// A granted handler that errors propagates that error — never a null push.
#[test]
fn a_failing_granted_handler_propagates_instead_of_null() {
    let program = dom_program();
    let boom = || {
        let mut caps = caps_with(true);
        // Re-registering the same gate replaces the stub.
        caps.register(Box::new(Boom));
        caps
    };
    let sched = run_with_caps(&program, &Quotas::default(), Some(&boom()))
        .expect_err("a failing handler must propagate")
        .to_string();
    assert!(
        sched.contains("dom_native.query") && sched.contains("boom"),
        "{sched}"
    );
    let mut pvm = PortableVm::new(program);
    pvm.set_host_caps(boom());
    let portable = pvm
        .run()
        .expect_err("PortableVm must propagate")
        .to_string();
    assert_eq!(portable, sched, "engines agree on the propagated error");
}

/// The registered stubs are unchanged: a granted `DOM_QUERY` pushes the
/// stub Map `{ok: true, kind: "query", echo: []}`.
#[test]
fn the_registered_stubs_still_answer() {
    let program = dom_program();
    let caps = || caps_with(true);
    let sched =
        run_with_caps(&program, &Quotas::default(), Some(&caps())).expect("granted must run");
    let Value::Map(m) = sched.stack.last().expect("stub pushed") else {
        panic!("expected the stub Map, got {:?}", sched.stack.last());
    };
    let borrowed = m.borrow();
    assert_eq!(borrowed.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(borrowed.get("kind"), Some(&Value::Str("query".to_string())));
    let mut pvm = PortableVm::new(dom_program());
    pvm.set_host_caps(caps());
    let portable = pvm.run().expect("granted must run");
    assert_eq!(
        portable.stack.last(),
        sched.stack.last(),
        "engines agree on the stub"
    );
}
