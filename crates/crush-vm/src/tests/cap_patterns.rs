//! CRUSH-164: pattern (`fs.*`) and expiry entries in `Quotas::allowed_caps`.

use crate::assembler::assemble;
use crate::host::{HostCap, HostCapSpec, HostCaps};
use crate::vm::{Quotas, Value, VmError, run_with_caps};
use std::time::{Duration, Instant};

fn q(entries: &[&str]) -> Quotas {
    Quotas { allowed_caps: Some(entries.iter().map(|s| s.to_string()).collect()), ..Quotas::default() }
}

#[test]
fn no_allowlist_permits_everything() {
    assert!(Quotas::default().cap_allowed("anything.at_all"));
}

#[test]
fn exact_names_are_unchanged() {
    let quotas = q(&["fs.read", "io.print"]);
    assert!(quotas.cap_allowed("fs.read"));
    assert!(quotas.cap_allowed("io.print"));
    assert!(!quotas.cap_allowed("fs.list"));
    assert!(!quotas.cap_allowed("fs.read2"));
    assert!(!q(&[]).cap_allowed("fs.read"));
}

#[test]
fn wildcard_matches_children() {
    let quotas = q(&["fs.*"]);
    assert!(quotas.cap_allowed("fs.read"));
    assert!(quotas.cap_allowed("fs.list"));
    assert!(quotas.cap_allowed("fs.a.b"));
}

#[test]
fn wildcard_does_not_match_siblings_or_the_bare_prefix() {
    let quotas = q(&["fs.*"]);
    assert!(!quotas.cap_allowed("fsx.read"));
    assert!(!quotas.cap_allowed("fs"));
    assert!(!quotas.cap_allowed("fs."));
    assert!(!quotas.cap_allowed("net.get"));
}

#[test]
fn bare_star_is_inert_without_opt_in() {
    let quotas = q(&["*"]);
    assert!(!quotas.cap_allowed("fs.read"));
    assert!(!quotas.cap_allowed("io.print"));
}

#[test]
fn bare_star_matches_all_with_opt_in() {
    let quotas = Quotas { allow_any_cap: true, ..q(&["*"]) };
    assert!(quotas.cap_allowed("fs.read"));
    assert!(quotas.cap_allowed("io.print"));
    // The opt-in alone, with no `*` entry, widens nothing.
    let quotas = Quotas { allow_any_cap: true, ..q(&["io.print"]) };
    assert!(!quotas.cap_allowed("fs.read"));
}

#[test]
fn expired_entries_match_nothing() {
    let past = Instant::now() - Duration::from_secs(1);
    let quotas = Quotas::default().allow_cap_until("fs.*", past).allow_cap_until("io.print", past);
    assert!(!quotas.cap_allowed("fs.read"));
    assert!(!quotas.cap_allowed("io.print"));
}

#[test]
fn unexpired_entries_match_and_expiry_is_per_entry() {
    let future = Instant::now() + Duration::from_secs(3600);
    let past = Instant::now() - Duration::from_secs(1);
    let mut quotas = Quotas::default().allow_cap_until("fs.*", future).allow_cap_until("net.*", past);
    assert!(quotas.cap_allowed("fs.read"));
    assert!(!quotas.cap_allowed("net.get"));
    // A second, non-expiring entry still covers what an expired one dropped.
    quotas.allowed_caps.as_mut().unwrap().push("net.get".into());
    assert!(quotas.cap_allowed("net.get"));
}

struct FsRead;

impl HostCap for FsRead {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "fs.read".into(), argc: Some(0), returns: true }
    }
    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        Ok(Some(Value::Int(7)))
    }
}

fn run_fs_read(quotas: &Quotas) -> Result<crate::vm::VmResult, VmError> {
    let mut caps = HostCaps::new();
    caps.register(Box::new(FsRead));
    let prog = assemble("CAP_CALL \"fs.read\" 0\nHALT", Some(&["fs.read"]), None).unwrap();
    run_with_caps(&prog, quotas, Some(&caps))
}

#[test]
fn end_to_end_program_runs_under_fs_wildcard() {
    let r = run_fs_read(&q(&["fs.*"])).expect("fs.* must allow fs.read");
    assert_eq!(r.stack.last(), Some(&Value::Int(7)));
}

#[test]
fn end_to_end_program_refused_under_net_wildcard() {
    match run_fs_read(&q(&["net.*"])) {
        Err(VmError::CapDenied(cap)) => assert_eq!(cap, "fs.read"),
        other => panic!("expected CapDenied(fs.read), got {other:?}"),
    }
}

#[test]
fn end_to_end_expired_wildcard_refuses() {
    let quotas = Quotas::default().allow_cap_until("fs.*", Instant::now() - Duration::from_secs(1));
    assert!(matches!(run_fs_read(&quotas), Err(VmError::CapDenied(_))));
}
