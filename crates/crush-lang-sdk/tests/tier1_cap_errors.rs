//! #464 / ruling 5 (#427): a granted capability that runs and fails
//! (`CapFailed`) is fatal on every Tier-1 engine, like `CapDenied`. A
//! `try`/`catch` around it does not swallow it.
//!
//! The capability is `fs.read` of a missing file, granted through the
//! in-memory file system on the scheduler VM and PortableVm. FastVM has no
//! bridge to the host caps, so it gets a `fs.read` capability that fails the
//! same way.
//!
//! Before #464 the scheduler and PortableVm caught the failure (the `catch`
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
use crush_vm::memory::Arena;
use crush_vm::vm::VmError;
use crush_vm::{PortableVm, Quotas, RuntimeValue};

/// One engine's outcome, by class: `Ok` with `main`'s value, or the class of
/// the error and its display text.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Ok(String),
    CapFailed(String),
    Other(String),
}

fn vm_outcome(r: Result<crush_vm::VmResult, VmError>) -> Outcome {
    match r {
        Ok(res) => Outcome::Ok(format!(
            "{:?}",
            res.return_value.as_ref().or(res.stack.last())
        )),
        Err(e @ VmError::CapFailed { .. }) => Outcome::CapFailed(e.to_string()),
        Err(e) => Outcome::Other(format!("{e:?}")),
    }
}

fn fast_outcome(y: FastYield) -> Outcome {
    match y {
        FastYield::Finished(v) => Outcome::Ok(format!("{v:?}")),
        FastYield::Error(e @ FastError::CapFailed { .. }) => Outcome::CapFailed(e.to_string()),
        other => Outcome::Other(format!("{other:?}")),
    }
}

fn scheduler(source: &str) -> Outcome {
    let prog = crush_lang_sdk::compile::compile_crush_source(source).expect("compile");
    let caps = HostCapsBuilder::new().fs_memory().build();
    vm_outcome(crush_vm::run_with_caps(
        &prog,
        &Quotas::default(),
        Some(&caps),
    ))
}

fn portable(source: &str) -> Outcome {
    let prog = crush_lang_sdk::compile::compile_crush_source(source).expect("compile");
    let mut vm = PortableVm::new(prog);
    vm.set_host_caps(HostCapsBuilder::new().fs_memory().build());
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

fn fastvm(source: &str) -> Outcome {
    let casm = crush_frontend::compile_crush_source(source).expect("frontend");
    let lowered = lower_program(&casm).expect("lower");
    let mut names: Vec<(&String, &u32)> = lowered.symbols.capabilities.iter().collect();
    names.sort_by_key(|(_, idx)| **idx);
    let caps: Vec<Arc<dyn Capability>> = names
        .into_iter()
        .map(|(name, _)| {
            assert_eq!(name, "fs.read", "unexpected capability {name}");
            Arc::new(MissingFile) as Arc<dyn Capability>
        })
        .collect();
    let mut vm = FastVM::new(lowered, caps, Arc::new(NoHal));
    fast_outcome(vm.run(100_000))
}

const BARE: &str = r#"fn main() {
    let c = fs.read("missing.txt")
    return 0
}
"#;

const IN_TRY: &str = r#"fn main() {
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
    assert_cap_failed("scheduler", "bare", scheduler(BARE));
}

#[test]
fn portable_cap_failure_is_cap_failed() {
    assert_cap_failed("PortableVm", "bare", portable(BARE));
}

#[test]
fn fastvm_cap_failure_is_cap_failed() {
    assert_cap_failed("FastVM", "bare", fastvm(BARE));
}

#[test]
fn scheduler_try_does_not_catch_cap_failed() {
    assert_cap_failed("scheduler", "in try", scheduler(IN_TRY));
}

#[test]
fn portable_try_does_not_catch_cap_failed() {
    assert_cap_failed("PortableVm", "in try", portable(IN_TRY));
}

#[test]
fn fastvm_try_does_not_catch_cap_failed() {
    assert_cap_failed("FastVM", "in try", fastvm(IN_TRY));
}
