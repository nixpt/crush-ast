//! CRUSH-32: `ai_native.*` host capability surface.
//!
//! 10 deterministic-stub caps for the AI opcodes. The 7 f49ece5 AOT stubs
//! were merged before this ticket; CRUSH-32 adds the surface so they (and
//! the 3 new variants `goal_declaration`, `progress_update`,
//! `knowledge_sharing`) can resolve to a real `HostCap` rather than just
//! pushing `Value::Null` in the scheduler/portable_vm/fastvm tiers.
//!
//! Each cap's `call(...)` returns
//! `Value::Map({ok: true, kind: "<name>", echo: <args>})` so the
//! `crush-diff` differential harness can assert byte-for-byte parity
//! across scheduler, portable_vm, fastvm, AOT-Rust, and AOT-C output.
//!
//! The Map is the runtime `Value::Map(Rc<RefCell<HashMap<String, Value>>>)`,
//! NOT a serialized JSON blob — callers iterate it with `m.borrow().get(&k)`
//! matching the existing `crush-lang-sdk::codebase` cap-shape idiom
//! (`crates/crush-lang-sdk/src/codebase.rs:750+`).
//!
//! OUT OF SCOPE (CRUSH-32 ticket): real AI backends (LLM, agents, etc).
//! These stubs are placeholders that produce self-documenting output
//! until the real backends land in a later milestone. The surface shape
//! AND the gate names (ai_native.<kind>) are stable; the implementation
//! under those gates is what later milestones will swap.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// All 10 `ai_native.<kind>` caps registered.
///
/// Idempotent in practice: `HostCaps::register` re-keys by name, so a
/// second call replaces any prior handler for the same gate. Used by
/// the `HostCapsBuilder::ai_native(bool)` toggle and by tests.
pub fn register(caps: &mut HostCaps) {
    caps.register(Box::new(AiNativeQueryCap));
    caps.register(Box::new(AiNativeSynthesizeCap));
    caps.register(Box::new(AiNativeAgentDelegationCap));
    caps.register(Box::new(AiNativeSemanticMatchCap));
    caps.register(Box::new(AiNativeLearningLoopCap));
    caps.register(Box::new(AiNativeContextAwareCap));
    caps.register(Box::new(AiNativeToolchainCap));
    caps.register(Box::new(AiNativeGoalDeclarationCap));
    caps.register(Box::new(AiNativeProgressUpdateCap));
    caps.register(Box::new(AiNativeKnowledgeSharingCap));
}

/// All 10 kind strings in registration order. Reused by tests + the
/// `HostCapsBuilder`'s `ai_native(bool)` toggle — single source of truth
/// for "which gates DO exist on this surface" (no drift between the
/// macro-generated structs below and the public docs/HELP text).
pub const KINDS: &[&str] = &[
    "query",
    "synthesize",
    "agent_delegation",
    "semantic_match",
    "learning_loop",
    "context_aware",
    "toolchain",
    "goal_declaration",
    "progress_update",
    "knowledge_sharing",
];

/// Build the stub Map: `{ok: true, kind: "<name>", echo: <args>}`.
///
/// Self-documenting at REPL inspect time (`ai_native.query() == ...` shows
/// the structure), and trivially diffable across adjacent execution tiers
/// in `crush-diff` — both properties the ticket explicitly calls for as
/// "Tier agreement" assertions. The `echo` field is wrapped as a
/// `Value::Array` (one element per arg) — that mirrors what the existing
/// `codebase.modules` cap does for its row-per-module shape, so callers
/// comparing `ai_native.query()` output to `codebase.modules()` output
/// use the same descendant-pattern.
fn stub_map(kind: &str, args: &[Value]) -> Value {
    let mut obj: HashMap<String, Value> = HashMap::with_capacity(3);
    obj.insert("ok".to_string(), Value::Bool(true));
    obj.insert("kind".to_string(), Value::Str(kind.to_string()));
    obj.insert(
        "echo".to_string(),
        Value::new_array(args.to_vec()),
    );
    Value::Map(Rc::new(RefCell::new(obj)))
}

/// Single-cap declaration. `argc: Some(0)` (each AI cap takes no stack
/// args today; the JSON payload is carried inline at bytecode parse time,
/// not via VM stack — real backends can override this later by registering
/// a different impl under the same gate); `returns: true` (always pushes
/// the stub Map onto the VM stack).
///
/// `HostCap::spec` returns OWNED `HostCapSpec` (per the actual trait
/// signature in `crush-vm/src/host.rs`), so no static caching / `Box::leak`
/// dance needed — each call produces a fresh owned value.
macro_rules! ai_native_cap {
    ($name:ident, $kind:literal) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: format!("ai_native.{}", $kind),
                    argc: Some(0),
                    returns: true,
                }
            }
            fn call(
                &self,
                args: Vec<Value>,
            ) -> Result<Option<Value>, String> {
                Ok(Some(stub_map($kind, &args)))
            }
        }
    };
}

ai_native_cap!(AiNativeQueryCap, "query");
ai_native_cap!(AiNativeSynthesizeCap, "synthesize");
ai_native_cap!(AiNativeAgentDelegationCap, "agent_delegation");
ai_native_cap!(AiNativeSemanticMatchCap, "semantic_match");
ai_native_cap!(AiNativeLearningLoopCap, "learning_loop");
ai_native_cap!(AiNativeContextAwareCap, "context_aware");
ai_native_cap!(AiNativeToolchainCap, "toolchain");
ai_native_cap!(AiNativeGoalDeclarationCap, "goal_declaration");
ai_native_cap!(AiNativeProgressUpdateCap, "progress_update");
ai_native_cap!(AiNativeKnowledgeSharingCap, "knowledge_sharing");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_10_caps_have_distinct_spec_names() {
        let caps: Vec<&dyn HostCap> = vec![
            &AiNativeQueryCap,
            &AiNativeSynthesizeCap,
            &AiNativeAgentDelegationCap,
            &AiNativeSemanticMatchCap,
            &AiNativeLearningLoopCap,
            &AiNativeContextAwareCap,
            &AiNativeToolchainCap,
            &AiNativeGoalDeclarationCap,
            &AiNativeProgressUpdateCap,
            &AiNativeKnowledgeSharingCap,
        ];
        let names: std::collections::HashSet<_> = caps
            .iter()
            .map(|c| c.spec().name.clone())
            .collect();
        assert_eq!(
            names.len(),
            10,
            "all 10 ai_native caps must have distinct spec().name values"
        );
    }

    #[test]
    fn spec_names_match_kinds_constant_in_order() {
        // KINDS is the hand-maintained public enumeration; the cap structs
        // are macro-generated and the macro takes the kind string literally
        // in their spec().name. If anyone reorders KINDS or renames a kind,
        // this test fails.
        let caps: Vec<&dyn HostCap> = vec![
            &AiNativeQueryCap,
            &AiNativeSynthesizeCap,
            &AiNativeAgentDelegationCap,
            &AiNativeSemanticMatchCap,
            &AiNativeLearningLoopCap,
            &AiNativeContextAwareCap,
            &AiNativeToolchainCap,
            &AiNativeGoalDeclarationCap,
            &AiNativeProgressUpdateCap,
            &AiNativeKnowledgeSharingCap,
        ];
        assert_eq!(caps.len(), KINDS.len());
        for (i, kind) in KINDS.iter().enumerate() {
            assert_eq!(caps[i].spec().name, format!("ai_native.{kind}"));
        }
    }

    #[test]
    fn stub_map_includes_kind_ok_echo() {
        let inputs = vec![Value::Str("hello".into())];
        let v = stub_map("query", &inputs);
        let Value::Map(m) = v else {
            panic!("expected Value::Map");
        };
        let borrowed = m.borrow();
        assert_eq!(borrowed.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            borrowed.get("kind"),
            Some(&Value::Str("query".to_string()))
        );
        assert!(borrowed.contains_key("echo"), "echo key must be set");
        // Echo is a Value::Array containing the input args.
        // Value::Array carries `Rc<RefCell<Vec<Value>>>` (parallels `Value::Map`),
        // so we borrow() before reading the inner Vec.
        match borrowed.get("echo") {
            Some(Value::Array(arr)) => {
                assert_eq!(arr.borrow().as_slice(), inputs.as_slice());
            }
            other => panic!("expected echo: Value::Array, got {other:?}"),
        }
    }

    #[test]
    fn stub_map_with_no_args_produces_empty_echo_array() {
        let v = stub_map("synthesize", &[]);
        let Value::Map(m) = v else { panic!("expected Map") };
        let borrowed = m.borrow();
        // `Value::new_array(vec![])` — the type-name `Array` (not `Vec`)
        // is canonical for crush-runtime; verify by shape alone here.
        match borrowed.get("echo") {
            Some(Value::Array(_arr)) => {}
            other => panic!("expected empty echo array, got {other:?}"),
        }
    }

    #[test]
    fn call_returns_some_map_with_kind_field() {
        let out = AiNativeSynthesizeCap.call(vec![]).unwrap().unwrap();
        let Value::Map(m) = out else {
            panic!("expected Map");
        };
        assert_eq!(
            m.borrow().get("kind"),
            Some(&Value::Str("synthesize".to_string()))
        );
    }

    #[test]
    fn register_inserts_all_10_handlers() {
        let mut caps = HostCaps::new();
        register(&mut caps);
        for kind in KINDS {
            let name = format!("ai_native.{kind}");
            assert!(
                caps.get(&name).is_some(),
                "register() missed cap `{name}`",
            );
        }
    }

    #[test]
    fn kinds_constant_lists_all_ten() {
        assert_eq!(KINDS.len(), 10);
        assert!(KINDS.contains(&"query"));
        assert!(KINDS.contains(&"synthesize"));
        assert!(KINDS.contains(&"knowledge_sharing"));
        assert!(KINDS.contains(&"goal_declaration"));
        assert!(KINDS.contains(&"progress_update"));
    }

    #[test]
    fn spec_includes_argc_zero_and_returns_true() {
        let spec = AiNativeQueryCap.spec();
        assert_eq!(spec.argc, Some(0));
        assert!(spec.returns);
        assert_eq!(spec.name, "ai_native.query");
    }
}
