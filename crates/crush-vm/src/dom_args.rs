//! GitHub issue #378 (sibling of CRUSH-234's AI fix): the fail-closed
//! contract for the ten `dom_native.*` opcodes.
//!
//! Mirrors `crate::ai_args`: a DOM opcode with no registered handler is a
//! loud, named `VmError::CapDenied` — not a silent `Value::Null` — and a
//! granted handler's error propagates instead of being flattened into
//! null. The scheduler and PortableVm both go through [`dispatch`], so the
//! two engines cannot disagree about what an ungranted or failing DOM cap
//! does.

use crate::host::{HostCapError, HostCaps};
use crate::vm::{Quotas, Value, VmError};

/// The error for a DOM op with no registered handler / grant. One message
/// for the scheduler, PortableVm and FastVM embedders.
pub fn not_granted(kind: &str) -> VmError {
    VmError::CapDenied(format!(
        "dom_native.{kind}: DOM op '{kind}' not granted / no DOM host registered \
         (register a provider under `dom_native.{kind}` via HostCaps::register; \
         the stubs are crush_lang_sdk::dom_native::register)"
    ))
}

/// Dispatch `dom_native.<kind>` through `host_caps`.
///
/// Ungranted (no handler registered) is a loud `VmError::CapDenied` naming
/// the op (GitHub issue #378, mirroring CRUSH-234 for the AI opcodes): a
/// silently-null DOM query is worse than an error. The same message is
/// produced on every engine. A granted handler is arity-checked against
/// its spec and run with the wall-clock deadline, like any `CAP_CALL`; its
/// error propagates — the old `.ok().flatten().unwrap_or(Null)` swallowed
/// it.
///
/// The handler is still called with no VM-stack args: the DOM stubs
/// declare `argc: Some(0)` and the opcode's string operand (the selector)
/// is not passed to handlers on any tier today. Passing it is the
/// CRUSH-252 real-provider work — all five tiers would have to agree,
/// including FastVM's fixed `echo: []` shape and the AOT codegens — not a
/// small change, so this fix keeps the empty-args call and documents the
/// gap.
pub fn dispatch(
    kind: &str,
    host_caps: Option<&HostCaps>,
    quotas: &Quotas,
    deadline_ms: u64,
) -> Result<Value, VmError> {
    let gate = format!("dom_native.{kind}");
    let Some(handler) = host_caps.and_then(|h| h.get(&gate)) else {
        return Err(not_granted(kind));
    };
    // The embedder's allowlist applies as it does to CAP_CALL and EXEC_LANG.
    // Not the declaration check: the compiler does not declare `dom_native.*`.
    crate::scheduler::check_cap_permitted(&gate, true, quotas)?;
    let args: Vec<Value> = Vec::new();
    if let Some(expected) = handler.spec().argc
        && args.len() != expected
    {
        return Err(VmError::CapArity {
            cap: gate,
            expected,
            got: args.len(),
        });
    }
    match handler.call_with_deadline(args, deadline_ms) {
        Ok(v) => Ok(v.unwrap_or(Value::Null)),
        Err(HostCapError::Timeout) => Err(VmError::CapTimeout {
            cap: gate,
            limit_ms: deadline_ms,
        }),
        Err(HostCapError::Message(message)) => Err(VmError::CapFailed { cap: gate, message }),
        Err(HostCapError::Exit(code)) => Err(VmError::Exit(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostCap, HostCapSpec};

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

    struct WrongArity;
    impl HostCap for WrongArity {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec {
                name: "dom_native.query".into(),
                argc: Some(2),
                returns: true,
            }
        }
        fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
            Ok(Some(Value::Int(1)))
        }
    }

    #[test]
    fn ungranted_is_a_named_error_and_never_calls() {
        let caps = HostCaps::new();
        let err = dispatch("query", Some(&caps), &Quotas::default(), 1000).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, VmError::CapDenied(_)), "{msg}");
        assert!(
            msg.contains("dom_native.query") && msg.contains("not granted"),
            "{msg}"
        );
        let none = dispatch("query", None, &Quotas::default(), 1000).unwrap_err();
        assert_eq!(none.to_string(), msg);
    }

    #[test]
    fn a_failing_handler_propagates_instead_of_becoming_null() {
        let mut caps = HostCaps::new();
        caps.register(Box::new(Boom));
        let err = dispatch("query", Some(&caps), &Quotas::default(), 1000).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("dom_native.query") && msg.contains("boom"),
            "{msg}"
        );
        // A granted capability that ran and failed: catchable `CapFailed`,
        // like the same failure through CAP_CALL, not a security refusal.
        assert!(matches!(err, VmError::CapFailed { .. }), "{err:?}");
    }

    #[test]
    fn arity_is_checked_against_the_spec() {
        let mut caps = HostCaps::new();
        caps.register(Box::new(WrongArity));
        let err = dispatch("query", Some(&caps), &Quotas::default(), 1000).unwrap_err();
        assert!(
            matches!(
                err,
                VmError::CapArity {
                    expected: 2,
                    got: 0,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_returning_handler_yields_its_value() {
        struct Ok_;
        impl HostCap for Ok_ {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: "dom_native.query".into(),
                    argc: Some(0),
                    returns: true,
                }
            }
            fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
                Ok(Some(Value::Int(7)))
            }
        }
        let mut caps = HostCaps::new();
        caps.register(Box::new(Ok_));
        assert_eq!(
            dispatch("query", Some(&caps), &Quotas::default(), 1000).unwrap(),
            Value::Int(7)
        );
    }

    #[test]
    fn the_embedder_allowlist_refuses_a_granted_op_without_calling_it() {
        let mut caps = HostCaps::new();
        caps.register(Box::new(Boom));
        let quotas = Quotas {
            allowed_caps: Some(vec!["io.print".into()]),
            ..Default::default()
        };
        // Boom would fail with "boom"; refused first, it never runs.
        let err = dispatch("query", Some(&caps), &quotas, 1000).unwrap_err();
        assert!(
            matches!(&err, VmError::CapDenied(cap) if cap == "dom_native.query"),
            "{err:?}"
        );
        let allowed = Quotas {
            allowed_caps: Some(vec!["dom_native.query".into()]),
            ..Default::default()
        };
        let err = dispatch("query", Some(&caps), &allowed, 1000).unwrap_err();
        assert!(matches!(err, VmError::CapFailed { .. }), "{err:?}");
    }
}
