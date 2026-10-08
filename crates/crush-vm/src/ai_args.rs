//! CRUSH-156: the one argument contract for the ten `ai_native.*` opcodes.
//!
//! An AI opcode carries two kinds of argument:
//!
//! 1. a **payload** — the compiled JSON object the frontend builds from the
//!    CAST node (`{query, result_type, context}`, `{tools, strategy,
//!    error_handling}`, …), stored as the opcode's string operand;
//! 2. **stack operands** — values the frontend compiled onto the VM stack
//!    just before the opcode (the wrapped expression of `context_aware`, the
//!    target of `semantic_match`, the context refs and examples of
//!    `synthesize`). Their count rides in the payload under
//!    [`STACK_ARGS_KEY`]; absent means 0.
//!
//! The `ai_native.<kind>` capability is called with
//! `[payload, operand_1, …, operand_n]`: the payload as a `Value::Map`
//! (with the `stack_args` bookkeeping key removed) followed by the operands
//! in push order. The scheduler, PortableVm and FastVM's
//! `resolve_host_request` all go through this module, so the three engines
//! cannot disagree about what a cap receives.

use crate::host::{HostCapError, HostCaps};
use crate::vm::{Value, VmError};

/// Payload key holding the number of stack operands the opcode consumes.
pub const STACK_ARGS_KEY: &str = "stack_args";

/// Parse an AI opcode's string operand. An empty operand (hand-written
/// assembly predating CRUSH-156) is an empty payload.
pub fn parse_payload(raw: &str) -> Result<serde_json::Value, VmError> {
    if raw.trim().is_empty() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    serde_json::from_str(raw)
        .map_err(|e| VmError::UnknownCap(format!("ai_native: invalid payload JSON: {e}")))
}

/// Number of stack operands the opcode consumes (`payload.stack_args`).
pub fn stack_argc(payload: &serde_json::Value) -> usize {
    payload
        .get(STACK_ARGS_KEY)
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize
}

/// Build the cap's argument vector: `[payload, operands…]`.
pub fn cap_args(payload: &serde_json::Value, operands: Vec<Value>) -> Vec<Value> {
    let mut payload = payload.clone();
    if let Some(obj) = payload.as_object_mut() {
        obj.remove(STACK_ARGS_KEY);
    }
    let payload: Value = serde_json::from_value(payload).unwrap_or(Value::Null);
    let mut args = Vec::with_capacity(1 + operands.len());
    args.push(payload);
    args.extend(operands);
    args
}

/// Dispatch `ai_native.<kind>` through `host_caps`.
///
/// Ungranted (no handler registered) yields `Value::Null` — the pre-CRUSH-32
/// default; the operands are still consumed by the caller, so the stack stays
/// balanced either way. A granted handler is arity-checked against its spec
/// and run with the wall-clock deadline, like any `CAP_CALL`.
pub fn dispatch(
    kind: &str,
    payload: &serde_json::Value,
    operands: Vec<Value>,
    host_caps: Option<&HostCaps>,
    deadline_ms: u64,
) -> Result<Value, VmError> {
    let gate = format!("ai_native.{kind}");
    let Some(handler) = host_caps.and_then(|h| h.get(&gate)) else {
        return Ok(Value::Null);
    };
    let args = cap_args(payload, operands);
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
        Err(HostCapError::Message(msg)) => Err(VmError::UnknownCap(format!("{gate}: {msg}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostCap, HostCapSpec};
    use std::sync::{Arc, Mutex};

    struct Recorder {
        argc: Option<usize>,
        // `Value` is not `Send`; record the args as JSON.
        seen: Arc<Mutex<serde_json::Value>>,
    }

    impl HostCap for Recorder {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec {
                name: "ai_native.semantic_match".into(),
                argc: self.argc,
                returns: true,
            }
        }
        fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
            *self.seen.lock().unwrap() = serde_json::to_value(&args).unwrap();
            Ok(Some(Value::Int(1)))
        }
    }

    #[test]
    fn payload_then_operands_in_push_order() {
        let seen = Arc::new(Mutex::new(serde_json::Value::Null));
        let mut caps = HostCaps::new();
        caps.register(Box::new(Recorder {
            argc: Some(2),
            seen: seen.clone(),
        }));
        let payload = serde_json::json!({"concept": "greeting", "stack_args": 1});
        let out = dispatch(
            "semantic_match",
            &payload,
            vec![Value::Str("hi".into())],
            Some(&caps),
            1000,
        )
        .unwrap();
        assert_eq!(out, Value::Int(1));
        // payload first (bookkeeping key stripped), then the operand.
        assert_eq!(
            *seen.lock().unwrap(),
            serde_json::json!([{"concept": "greeting"}, "hi"])
        );
    }

    #[test]
    fn arity_is_checked_against_the_spec() {
        let mut caps = HostCaps::new();
        caps.register(Box::new(Recorder {
            argc: Some(2),
            seen: Default::default(),
        }));
        let err = dispatch(
            "semantic_match",
            &serde_json::json!({}),
            vec![],
            Some(&caps),
            1000,
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                VmError::CapArity {
                    expected: 2,
                    got: 1,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn ungranted_is_null_and_never_calls() {
        let caps = HostCaps::new();
        let out = dispatch(
            "query",
            &serde_json::json!({"query": "x"}),
            vec![],
            Some(&caps),
            1000,
        )
        .unwrap();
        assert_eq!(out, Value::Null);
        assert_eq!(
            dispatch("query", &serde_json::json!({}), vec![], None, 1000).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn stack_argc_and_empty_payload() {
        assert_eq!(stack_argc(&serde_json::json!({"stack_args": 3})), 3);
        assert_eq!(stack_argc(&serde_json::json!({})), 0);
        assert_eq!(parse_payload("").unwrap(), serde_json::json!({}));
        assert!(parse_payload("{not json").is_err());
    }
}
