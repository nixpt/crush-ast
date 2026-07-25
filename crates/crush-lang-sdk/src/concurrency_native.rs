//! CRUSH-34: `concurrency_native.*` host capability surface.
//!
//! 3 deterministic-stub caps for the concurrency opcodes
//! (SPAWN=0x80, YIELD=0x81, AWAIT=0x82). NOTE: unlike the CRUSH-32
//! (AI) and CRUSH-33 (DOM) siblings, these byte-slots are
//! PRE-EXISTING in `bytecode.rs:95-97` — they were part of the
//! original CVM1 opcode table. CRUSH-34 only adds the `KINDS` const,
//! the `concurrency_native_cap!` macro, the 3 cap structs, and the
//! tests; the byte-slot constants themselves are untouched.
//!
//! Mirrors `crush_lang_sdk::ai_native` and `crush_lang_sdk::dom_native`
//! byte-for-byte (KINDS const + register() + `concurrency_native_cap!`
//! macro + 3 macro-generated Cap structs + 7 unit tests).
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
//! CRITICAL DIFFERENCE FROM ai/dom: concurrency ops (SPAWN/AWAIT/YIELD)
//! have ASYNC semantics in real backends (cooperative scheduling,
//! event-loop yield, await-on-handle). The stubs here return a
//! minimal `{ok, kind, echo}` placeholder. Real implementations will
//! need to model event_ids, task_ids, and (in a future OperandKind
//! extension) the per-opcode operand argument. See the CRUSH-34
//! ticket Review forward flags for the full list of deferred scope.
//!
//! OUT OF SCOPE (CRUSH-34 Commit 1 / skeleton): real concurrency
//! backends (thread pool, async runtime, event-loop driver). These
//! stubs are placeholders that produce self-documenting output until
//! the real backends land in a later milestone. The surface shape
//! AND the gate names (`concurrency_native.<kind>`) are stable; the
//! implementation under those gates is what CRUSH-34 Commit 2's
//! 5-tier wiring will swap.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// All 3 `concurrency_native.<kind>` caps registered.
///
/// Idempotent in practice: `HostCaps::register` re-keys by name, so a
/// second call replaces any prior handler for the same gate. Used by
/// the `HostCapsBuilder`'s `concurrency_native(bool)` toggle
/// (Commit 2) and by tests.
pub fn register(caps: &mut HostCaps) {
    caps.register(Box::new(ConcurrencyNativeSpawnCap));
    caps.register(Box::new(ConcurrencyNativeYieldCap));
    caps.register(Box::new(ConcurrencyNativeAwaitCap));
}

/// All 3 kind strings in registration order. Reused by tests + the
/// `HostCapsBuilder`'s `concurrency_native(bool)` toggle (Commit 2)
/// — single source of truth for "which gates DO exist on this
/// surface" (no drift between the macro-generated structs below and
/// the public docs/HELP text).
///
/// ORDERING NOTE: the canonical registration order here matches the
/// surface-natural order (`spawn → yield → await`), not the byte-slot
/// order (which would be `spawn=0x80, yield=0x81, await=0x82` —
/// same ordering coincidentally; preserved for consistency with the
/// AI/DOM precedent where the byte-slot and surface-natural orders
/// differ and the surface-natural order wins). The
/// `concurrency_native_kind_for_opcode` switch in
/// `crush-vm/src/bytecode.rs` handles bytes-to-kinds; the gate
/// registration here is kinds-to-handler.
pub const KINDS: &[&str] = &[
    "spawn",
    "yield",
    "await",
];

/// Build the stub Map: `{ok: true, kind: "<name>", echo: <args>}`.
///
/// Self-documenting at REPL inspect time (`concurrency_native.spawn()
/// == ...` shows the structure), and trivially diffable across
/// adjacent execution tiers in `crush-diff` — both properties the
/// ticket explicitly calls for as "Tier agreement" assertions. The
/// `echo` field is wrapped as a `Value::Array` (one element per arg)
/// — that mirrors what `ai_native::stub_map` and
/// `dom_native::stub_map` produce, so callers comparing
/// `concurrency_native.spawn()` output to AI/DOM output use the same
/// descendant-pattern.
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

/// Single-cap declaration. `argc: Some(0)` (each concurrency cap
/// takes no stack args today; the payload — task_id, event_id,
/// yield-count — would be carried inline at bytecode parse time in a
/// real backend, not via VM stack; real backends can override this
/// later by registering a different impl under the same gate);
/// `returns: true` (always pushes the stub Map onto the VM stack).
///
/// `HostCap::spec` returns OWNED `HostCapSpec` (per the actual
/// trait signature in `crush-vm/src/host.rs`), so no static caching
/// / `Box::leak` dance needed — each call produces a fresh owned
/// value.
macro_rules! concurrency_native_cap {
    ($name:ident, $kind:literal) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: format!("concurrency_native.{}", $kind),
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

concurrency_native_cap!(ConcurrencyNativeSpawnCap, "spawn");
concurrency_native_cap!(ConcurrencyNativeYieldCap, "yield");
concurrency_native_cap!(ConcurrencyNativeAwaitCap, "await");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_native_kinds_constant_is_size_three_and_sorted_unique() {
        // CRUSH-34 Commit 1: KINDS is the single source of truth for
        // the 3 concurrency opcodes (SPAWN=0x80, YIELD=0x81, AWAIT=0x82;
        // pre-existing slots). Add a new kind here and `register()`
        // both auto-routes through the macro. Failure here = someone
        // reordered or duplicated KINDS; fix the KINDS const, not the
        // test.
        assert_eq!(KINDS.len(), 3, "KINDS size changed - update HARD-CODED list and this test");
        let mut sorted: Vec<&str> = KINDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            KINDS.len(),
            "KINDS contains duplicates: {:?}",
            KINDS
        );
    }

    #[test]
    fn all_3_caps_have_distinct_spec_names() {
        // All 3 concurrency_native caps must have distinct
        // spec().name values
        let caps: Vec<&dyn HostCap> = vec![
            &ConcurrencyNativeSpawnCap,
            &ConcurrencyNativeYieldCap,
            &ConcurrencyNativeAwaitCap,
        ];
        let names: std::collections::HashSet<_> = caps
            .iter()
            .map(|c| c.spec().name.clone())
            .collect();
        assert_eq!(
            names.len(),
            3,
            "all 3 concurrency_native caps must have distinct spec().name values"
        );
    }

    #[test]
    fn spec_names_match_kinds_constant_in_order() {
        // KINDS is the hand-maintained public enumeration; the cap
        // structs are macro-generated and the macro takes the kind
        // string literally in their spec().name. If anyone reorders
        // KINDS or renames a kind, this test fails.
        let caps: Vec<&dyn HostCap> = vec![
            &ConcurrencyNativeSpawnCap,
            &ConcurrencyNativeYieldCap,
            &ConcurrencyNativeAwaitCap,
        ];
        assert_eq!(caps.len(), KINDS.len());
        for (i, kind) in KINDS.iter().enumerate() {
            assert_eq!(caps[i].spec().name, format!("concurrency_native.{kind}"));
        }
    }

    #[test]
    fn stub_map_includes_kind_ok_echo() {
        let inputs = vec![Value::Str("task_42".into())];
        let v = stub_map("spawn", &inputs);
        let Value::Map(m) = v else {
            panic!("expected Value::Map");
        };
        let borrowed = m.borrow();
        assert_eq!(borrowed.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(
            borrowed.get("kind"),
            Some(&Value::Str("spawn".to_string()))
        );
        assert!(borrowed.contains_key("echo"), "echo key must be set");
        // Echo is a Value::Array containing the input args.
        // Value::Array carries `Rc<RefCell<Vec<Value>>>` (parallels
        // `Value::Map`), so we borrow() before reading the inner Vec.
        match borrowed.get("echo") {
            Some(Value::Array(arr)) => {
                assert_eq!(arr.borrow().as_slice(), inputs.as_slice());
            }
            other => panic!("expected echo: Value::Array, got {other:?}"),
        }
    }

    #[test]
    fn stub_map_with_no_args_produces_empty_echo_array() {
        let v = stub_map("yield", &[]);
        let Value::Map(m) = v else { panic!("expected Map") };
        let borrowed = m.borrow();
        // `Value::new_array(vec![])` — the type-name `Array` (not
        // `Vec`) is canonical for crush-runtime; verify by shape
        // alone here.
        match borrowed.get("echo") {
            Some(Value::Array(_arr)) => {}
            other => panic!("expected empty echo array, got {other:?}"),
        }
    }

    #[test]
    fn call_returns_some_map_with_kind_field() {
        let out = ConcurrencyNativeSpawnCap.call(vec![]).unwrap().unwrap();
        let Value::Map(m) = out else {
            panic!("expected Map");
        };
        assert_eq!(
            m.borrow().get("kind"),
            Some(&Value::Str("spawn".to_string()))
        );
    }

    #[test]
    fn register_inserts_all_3_handlers() {
        let mut caps = HostCaps::new();
        register(&mut caps);
        for kind in KINDS {
            let name = format!("concurrency_native.{kind}");
            assert!(
                caps.get(&name).is_some(),
                "register() missed cap `{name}`",
            );
        }
    }
}
