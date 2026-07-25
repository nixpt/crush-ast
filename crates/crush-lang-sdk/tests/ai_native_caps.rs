//! CRUSH-32: integration test for the `ai_native.*` capability surface.
//!
//! Verifies the host_caps builder surfaces all 10 gates and that calling
//! each returns a deterministic Map stub of the agreed shape. This is the
//! end-to-end wire through which the fastvm HostRequest escalations and
//! the AI-capable scheduler/portable_vm arms would resolve (real backends
//! out of CRUSH-32 scope, but the registry has to be correct today so
//! later milestones can drop in real impls without re-wiring tests).

use crush_lang_sdk::ai_native;
use crush_vm::vm::Value;
use crush_vm::HostCaps;

#[test]
fn register_inserts_all_ten_ai_native_gates() {
    let mut caps = HostCaps::new();
    ai_native::register(&mut caps);
    for kind in ai_native::KINDS {
        let name = format!("ai_native.{kind}");
        assert!(
            caps.get(&name).is_some(),
            "expected gate `{name}` after register()",
        );
    }
}

#[test]
fn register_yields_a_distinct_cap_per_kind() {
    let mut caps = HostCaps::new();
    ai_native::register(&mut caps);
    let names: std::collections::HashSet<_> = ai_native::KINDS
        .iter()
        .map(|k| format!("ai_native.{k}"))
        .filter_map(|name| caps.get(&name).map(|h| (name, h.spec().name.clone())))
        .collect();
    assert_eq!(names.len(), ai_native::KINDS.len());
}

#[test]
fn every_cap_call_returns_a_map_with_kind_and_ok() {
    let mut caps = HostCaps::new();
    ai_native::register(&mut caps);
    for kind in ai_native::KINDS {
        let name = format!("ai_native.{kind}");
        let handler = caps.get(&name).expect("registered above");
        let out = handler
            .call(vec![])
            .expect("stub never errors")
            .expect("returns Some");        match out {
            Value::Map(m) => {
                let borrowed = m.borrow();
                match borrowed.get("kind") {
                    Some(Value::Str(s)) => {
                        // `s.as_str(): &str`, but iterating `for kind in
                        // KINDS` (where `KINDS: &[&str]`) yields `&&str`
                        // — so we deref once to compare with `&str`.
                        // (Compiler suggested the same fix in round-4 review.)
                        assert!(
                            s.as_str() == *kind,
                            "kind.value must equal {kind} (got {s:?})",
                        );
                    }
                    other => panic!("expected kind: Value::Str, got {other:?}"),
                }
                match borrowed.get("ok") {
                    Some(Value::Bool(true)) => {}
                    other => panic!("expected ok: Value::Bool(true), got {other:?}"),
                }
                assert!(
                    borrowed.contains_key("echo"),
                    "echo must be set even if empty",
                );
            }
            other => panic!("expected Value::Map from call({name}), got {other:?}"),
        }
    }
}
