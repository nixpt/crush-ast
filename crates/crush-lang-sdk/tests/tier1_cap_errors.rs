//! #464: how a failing granted capability behaves on every Tier-1 engine
//! (scheduler, PortableVm, FastVM).
//!
//! - A refusal (`CapFailed`) is fatal, like `CapDenied` (ruling 5, #427). A
//!   `try`/`catch` around it does not swallow it. The case here is `fs.read`
//!   of a missing file.
//! - A call's own `timeout_ms` firing (`CapTimedOut`) is not a refusal
//!   (captain's ruling on #507/#508): `try`/`catch` catches it, the handler
//!   binds `<cap>: timed out after N ms` and the program carries on. The case
//!   here is a `test.slow` cap that always times out after 150 ms.
//!
//! The scheduler and PortableVm get the real in-memory `fs.read` plus
//! `test.slow`. FastVM has no bridge to the host caps, so it gets FastVM
//! capabilities that fail the same way.
//!
//! Before #464 the scheduler and PortableVm caught the refusal (the `catch`
//! ran and `main` returned 1), and FastVM pushed the error text as the call's
//! result and carried on: `main` returned 0 without the program ever seeing
//! an error.
//!
//! The error class is decided here from the engine's own error variant. The
//! shared `ErrorClass` mapping belongs to the strict Tier-1 harness (#453);
//! until that lands this file keeps its own.

use std::sync::Arc;

use crush_lang_sdk::HostCapsBuilder;
use crush_vm::fastvm::{Capability, FastError, FastVM, FastYield, Hal, lower_program};
use crush_vm::host::{HostCap, HostCapError, HostCapSpec, HostCaps};
use crush_vm::memory::Arena;
use crush_vm::vm::{Value, VmError};
use crush_vm::{PortableVm, Quotas, RuntimeValue};

const TIMEOUT_TEXT: &str = "test.slow: timed out after 150 ms";

/// One engine's outcome, by class: `Ok` with `main`'s value as text, or the
/// class of the error and its display text.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Ok(String),
    CapFailed(String),
    CapTimedOut(String),
    Other(String),
}

fn vm_value(v: Option<&Value>) -> String {
    match v {
        Some(Value::Str(s)) | Some(Value::Error(s)) => s.clone(),
        other => format!("{other:?}"),
    }
}

fn vm_outcome(r: Result<crush_vm::VmResult, VmError>) -> Outcome {
    match r {
        Ok(res) => Outcome::Ok(vm_value(res.return_value.as_ref().or(res.stack.last()))),
        Err(e @ VmError::CapFailed { .. }) => Outcome::CapFailed(e.to_string()),
        Err(e @ VmError::CapTimedOut { .. }) => Outcome::CapTimedOut(e.to_string()),
        Err(e) => Outcome::Other(format!("{e:?}")),
    }
}

fn fast_outcome(y: FastYield) -> Outcome {
    match y {
        FastYield::Finished(Some(RuntimeValue::String(s))) => Outcome::Ok(s),
        FastYield::Finished(v) => Outcome::Ok(format!("{v:?}")),
        FastYield::Error(e @ FastError::CapFailed { .. }) => Outcome::CapFailed(e.to_string()),
        FastYield::Error(e @ FastError::CapTimedOut { .. }) => Outcome::CapTimedOut(e.to_string()),
        other => Outcome::Other(format!("{other:?}")),
    }
}

/// `test.slow`: its own `timeout_ms` always fires, as `tcp.recv(c,
/// {timeout_ms: 150})` does when nothing arrives.
struct Slow;
impl HostCap for Slow {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "test.slow".to_string(),
            argc: Some(0),
            returns: true,
        }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Err(TIMEOUT_TEXT.to_string())
    }
    fn call_with_deadline(
        &self,
        _args: Vec<Value>,
        _deadline_ms: u64,
    ) -> Result<Option<Value>, HostCapError> {
        Err(HostCapError::TimedOut {
            timeout_ms: 150,
            message: TIMEOUT_TEXT.to_string(),
        })
    }
}

fn host_caps() -> HostCaps {
    let mut caps = HostCapsBuilder::new().fs_memory().build();
    caps.register(Box::new(Slow));
    caps
}

fn scheduler(source: &str) -> Outcome {
    let prog = crush_lang_sdk::compile::compile_crush_source(source).expect("compile");
    let caps = host_caps();
    vm_outcome(crush_vm::run_with_caps(
        &prog,
        &Quotas::default(),
        Some(&caps),
    ))
}

fn portable(source: &str) -> Outcome {
    let prog = crush_lang_sdk::compile::compile_crush_source(source).expect("compile");
    let mut vm = PortableVm::new(prog);
    vm.set_host_caps(host_caps());
    vm_outcome(vm.run())
}

#[derive(Debug)]
struct NoHal;
impl Hal for NoHal {}

/// FastVM's `fs.read`: always fails, as the host cap does for a missing file.
#[derive(Debug)]
struct MissingFile;
impl Capability for MissingFile {
    fn name(&self) -> &str {
        "fs.read"
    }
    fn call(
        &self,
        _arena: &mut Arena,
        _args: Vec<RuntimeValue>,
        _hal: Arc<dyn Hal>,
    ) -> anyhow::Result<RuntimeValue> {
        anyhow::bail!("fs.read missing.txt: No such file or directory")
    }
}

/// FastVM's `test.slow`: raises the same `HostCapError::TimedOut`.
#[derive(Debug)]
struct FastSlow;
impl Capability for FastSlow {
    fn name(&self) -> &str {
        "test.slow"
    }
    fn call(
        &self,
        _arena: &mut Arena,
        _args: Vec<RuntimeValue>,
        _hal: Arc<dyn Hal>,
    ) -> anyhow::Result<RuntimeValue> {
        Err(HostCapError::TimedOut {
            timeout_ms: 150,
            message: TIMEOUT_TEXT.to_string(),
        }
        .into())
    }
}

fn fastvm(source: &str) -> Outcome {
    let casm = crush_frontend::compile_crush_source(source).expect("frontend");
    let lowered = lower_program(&casm).expect("lower");
    let mut names: Vec<(&String, &u32)> = lowered.symbols.capabilities.iter().collect();
    names.sort_by_key(|(_, idx)| **idx);
    let caps: Vec<Arc<dyn Capability>> = names
        .into_iter()
        .map(|(name, _)| match name.as_str() {
            "fs.read" => Arc::new(MissingFile) as Arc<dyn Capability>,
            "test.slow" => Arc::new(FastSlow) as Arc<dyn Capability>,
            other => panic!("unexpected capability {other}"),
        })
        .collect();
    let mut vm = FastVM::new(lowered, caps, Arc::new(NoHal));
    fast_outcome(vm.run(100_000))
}

type Engine = (&'static str, fn(&str) -> Outcome);

const ENGINES: [Engine; 3] = [
    ("scheduler", scheduler),
    ("PortableVm", portable),
    ("FastVM", fastvm),
];

// ── a refusal is fatal ──────────────────────────────────────────────────────

const REFUSAL_BARE: &str = r#"fn main() {
    let c = fs.read("missing.txt")
    return 0
}
"#;

const REFUSAL_IN_TRY: &str = r#"fn main() {
    try {
        let c = fs.read("missing.txt")
    } catch e {
        return 1
    }
    return 0
}
"#;

fn assert_cap_failed(engine: &str, case: &str, got: Outcome) {
    match &got {
        Outcome::CapFailed(text) => assert!(
            text.starts_with("fs.read: ") && text.contains("missing.txt"),
            "{engine} ({case}): unexpected CapFailed text: {text}"
        ),
        _ => panic!("{engine} ({case}): expected a fatal CapFailed, got {got:?}"),
    }
}

#[test]
fn scheduler_cap_failure_is_cap_failed() {
    assert_cap_failed("scheduler", "bare", scheduler(REFUSAL_BARE));
}

#[test]
fn portable_cap_failure_is_cap_failed() {
    assert_cap_failed("PortableVm", "bare", portable(REFUSAL_BARE));
}

#[test]
fn fastvm_cap_failure_is_cap_failed() {
    assert_cap_failed("FastVM", "bare", fastvm(REFUSAL_BARE));
}

#[test]
fn scheduler_try_does_not_catch_cap_failed() {
    assert_cap_failed("scheduler", "in try", scheduler(REFUSAL_IN_TRY));
}

#[test]
fn portable_try_does_not_catch_cap_failed() {
    assert_cap_failed("PortableVm", "in try", portable(REFUSAL_IN_TRY));
}

#[test]
fn fastvm_try_does_not_catch_cap_failed() {
    assert_cap_failed("FastVM", "in try", fastvm(REFUSAL_IN_TRY));
}

// ── a timeout_ms that fired is catchable ────────────────────────────────────

const TIMEOUT_BARE: &str = r#"fn main() {
    let r = test.slow()
    return "not reached"
}
"#;

/// The handler binds the timeout's text and the program carries on to its
/// own `return`.
const TIMEOUT_IN_TRY: &str = r#"fn main() {
    let caught = "nothing"
    try {
        let r = test.slow()
        caught = "not reached"
    } catch e {
        caught = e
    }
    return caught
}
"#;

#[test]
fn a_timeout_without_try_ends_the_run_as_cap_timed_out_on_every_engine() {
    for (engine, run) in ENGINES {
        assert_eq!(
            run(TIMEOUT_BARE),
            Outcome::CapTimedOut(TIMEOUT_TEXT.to_string()),
            "{engine}"
        );
    }
}

#[test]
fn scheduler_try_catches_a_timeout() {
    assert_eq!(
        scheduler(TIMEOUT_IN_TRY),
        Outcome::Ok(TIMEOUT_TEXT.to_string())
    );
}

#[test]
fn portable_try_catches_a_timeout() {
    assert_eq!(
        portable(TIMEOUT_IN_TRY),
        Outcome::Ok(TIMEOUT_TEXT.to_string())
    );
}

#[test]
fn fastvm_try_catches_a_timeout() {
    assert_eq!(
        fastvm(TIMEOUT_IN_TRY),
        Outcome::Ok(TIMEOUT_TEXT.to_string())
    );
}
