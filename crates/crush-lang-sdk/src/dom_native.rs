//! CRUSH-33: `dom_native.*` host capability surface.
//!
//! 10 deterministic-stub caps for the DOM opcodes (slots 0x9A-0x9F + 0xB5-0xB8,
//! split to avoid the existing MATH/VEC range at 0xA0-0xA8). Mirrors
//! `crush_lang_sdk::ai_native` byte-for-byte (KINDS const + register() +
//! `dom_native_cap!` macro + 10 macro-generated Cap structs + 6 unit tests).
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
//! OUT OF SCOPE (CRUSH-33 Commit 1 / skeleton): real DOM backends (jsdom,
//! headless-chrome, browser-driver). These stubs are placeholders that
//! produce self-documenting output until the real backends land in a
//! later milestone. The surface shape AND the gate names
//! (`dom_native.<kind>`) are stable; the implementation under those gates
//! is what later milestones (and CRUSH-33 Commit 2's 5-tier wiring) will
//! swap. Commit 2 brings the per-tier `cap_call` path that picks up the
//! gate name from `crush_vm::bytecode::dom_native_kind_for_opcode`.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// All 10 `dom_native.<kind>` caps registered.
///
/// Idempotent in practice: `HostCaps::register` re-keys by name, so a
/// second call replaces any prior handler for the same gate. Used by
/// the `HostCapsBuilder`'s `dom_native(bool)` toggle (Commit 2) and by
/// tests.
pub fn register(caps: &mut HostCaps) {
    caps.register(Box::new(DomNativeQueryCap));
    caps.register(Box::new(DomNativeGetCap));
    caps.register(Box::new(DomNativeSetCap));
    caps.register(Box::new(DomNativeCreateCap));
    caps.register(Box::new(DomNativeRemoveCap));
    caps.register(Box::new(DomNativeChildCap));
    caps.register(Box::new(DomNativeParentCap));
    caps.register(Box::new(DomNativeAttrCap));
    caps.register(Box::new(DomNativeTextCap));
    caps.register(Box::new(DomNativeEventCap));
}

/// All 10 kind strings in registration order. Reused by tests + the
/// `HostCapsBuilder`'s `dom_native(bool)` toggle (Commit 2) — single
/// source of truth for "which gates DO exist on this surface" (no drift
/// between the macro-generated structs below and the public docs/HELP
/// text).
///
/// ORDERING NOTE: the slot ranges 0x9A-0x9F + 0xB5-0xB8 do not numerically
/// sort cleanly, so the canonical registration order here matches the
/// surface-natural order (`query → get → set → create → remove → child →
/// parent → attr → text → event`), not the byte-slot order. The
/// `dom_native_kind_for_opcode` switch in `crush-vm/src/bytecode.rs`
/// reverses bytes-to-kinds; the gate registration here is kinds-to-handler.
pub const KINDS: &[&str] = &[
    "query",
    "get",
    "set",
    "create",
    "remove",
    "child",
    "parent",
    "attr",
    "text",
    "event",
];

/// Build the stub Map: `{ok: true, kind: "<name>", echo: <args>}`.
///
/// Self-documenting at REPL inspect time (`dom_native.query() == ...`
/// shows the structure), and trivially diffable across adjacent
/// execution tiers in `crush-diff` — both properties the ticket
/// explicitly calls for as "Tier agreement" assertions. The `echo` field
/// is wrapped as a `Value::Array` (one element per arg) — that mirrors
/// what `ai_native::stub_map` produces for the AI stubs, so callers
/// comparing `dom_native.query()` to `ai_native.query()` output use the
/// same descendant-pattern.
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

/// Single-cap declaration. `argc: Some(0)` (each DOM cap takes no stack
/// args today; the payload is carried inline at bytecode parse time — a
/// CSS selector string for `query`, a node-id for `get`, etc. — not via
/// VM stack; real backends can override this later by registering a
/// different impl under the same gate); `returns: true` (always pushes
/// the stub Map onto the VM stack).
///
/// `HostCap::spec` returns OWNED `HostCapSpec` (per the actual trait
/// signature in `crush-vm/src/host.rs`), so no static caching /
/// `Box::leak` dance needed — each call produces a fresh owned value.
macro_rules! dom_native_cap {
    ($name:ident, $kind:literal) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: format!("dom_native.{}", $kind),
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

dom_native_cap!(DomNativeQueryCap, "query");
dom_native_cap!(DomNativeGetCap, "get");
dom_native_cap!(DomNativeSetCap, "set");
dom_native_cap!(DomNativeCreateCap, "create");
dom_native_cap!(DomNativeRemoveCap, "remove");
dom_native_cap!(DomNativeChildCap, "child");
dom_native_cap!(DomNativeParentCap, "parent");
dom_native_cap!(DomNativeAttrCap, "attr");
dom_native_cap!(DomNativeTextCap, "text");
dom_native_cap!(DomNativeEventCap, "event");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_10_caps_have_distinct_spec_names() {
        let caps: Vec<&dyn HostCap> = vec![
            &DomNativeQueryCap,
            &DomNativeGetCap,
            &DomNativeSetCap,
            &DomNativeCreateCap,
            &DomNativeRemoveCap,
            &DomNativeChildCap,
            &DomNativeParentCap,
            &DomNativeAttrCap,
            &DomNativeTextCap,
            &DomNativeEventCap,
        ];
        let names: std::collections::HashSet<_> = caps
            .iter()
            .map(|c| c.spec().name.clone())
            .collect();
        assert_eq!(
            names.len(),
            10,
            "all 10 dom_native caps must have distinct spec().name values"
        );
    }

    #[test]
    fn spec_names_match_kinds_constant_in_order() {
        // KINDS is the hand-maintained public enumeration; the cap structs
        // are macro-generated and the macro takes the kind string literally
        // in their spec().name. If anyone reorders KINDS or renames a kind,
        // this test fails.
        let caps: Vec<&dyn HostCap> = vec![
            &DomNativeQueryCap,
            &DomNativeGetCap,
            &DomNativeSetCap,
            &DomNativeCreateCap,
            &DomNativeRemoveCap,
            &DomNativeChildCap,
            &DomNativeParentCap,
            &DomNativeAttrCap,
            &DomNativeTextCap,
            &DomNativeEventCap,
        ];
        assert_eq!(caps.len(), KINDS.len());
        for (i, kind) in KINDS.iter().enumerate() {
            assert_eq!(caps[i].spec().name, format!("dom_native.{kind}"));
        }
    }

    #[test]
    fn stub_map_includes_kind_ok_echo() {
        let inputs = vec![Value::Str("div.user".into())];
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
        let v = stub_map("get", &[]);
        let Value::Map(m) = v else { panic!("expected Map") };
        let borrowed = m.borrow();
        // `Value::new_array(vec![])` -- the type-name `Array` (not `Vec`)
        // is canonical for crush-runtime; verify by shape alone here.
        match borrowed.get("echo") {
            Some(Value::Array(_arr)) => {}
            other => panic!("expected empty echo array, got {other:?}"),
        }
    }

    #[test]
    fn call_returns_some_map_with_kind_field() {
        let out = DomNativeGetCap.call(vec![]).unwrap().unwrap();
        let Value::Map(m) = out else {
            panic!("expected Map");
        };
        assert_eq!(
            m.borrow().get("kind"),
            Some(&Value::Str("get".to_string()))
        );
    }

    #[test]
    fn register_inserts_all_10_handlers() {
        let mut caps = HostCaps::new();
        register(&mut caps);
        for kind in KINDS {
            let name = format!("dom_native.{kind}");
            assert!(
                caps.get(&name).is_some(),
                "register() missed cap `{name}`",
            );
        }
    }
}
