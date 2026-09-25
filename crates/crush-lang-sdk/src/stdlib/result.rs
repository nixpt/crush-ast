//! `result.*` — explicit success / failure values.
//!
//! Ported from exosphere `core/base/stdlib/src/result.rs`. nanovm had a
//! dedicated `Object::Result { ok, value }`; CVM1 has no such variant, so a
//! result is a map `{ "ok": bool, "value": v }`. Only maps with exactly those
//! two keys (and a bool `ok`) are accepted as results.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::collections::HashMap;

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(ResultOkCap));
    caps.register(Box::new(ResultErrCap));
    caps.register(Box::new(ResultIsOkCap));
    caps.register(Box::new(ResultUnwrapCap));
}

fn make(ok: bool, value: Value) -> Value {
    let mut m = HashMap::new();
    m.insert("ok".to_string(), Value::Bool(ok));
    m.insert("value".to_string(), value);
    Value::new_map(m)
}

/// Split a result into `(ok, value)`.
fn open(cap: &str, v: &Value) -> Result<(bool, Value), String> {
    let not_a_result = || format!("{cap}: expected a result (from result.ok / result.err)");
    let Value::Map(m) = v else {
        return Err(not_a_result());
    };
    let m = m.borrow();
    match (m.len(), m.get("ok"), m.get("value")) {
        (2, Some(Value::Bool(ok)), Some(value)) => Ok((*ok, value.clone())),
        _ => Err(not_a_result()),
    }
}

std_cap!(ResultOkCap, "result.ok", Some(1), |args: &[Value]| {
    Ok(Some(make(true, args[0].clone())))
});

std_cap!(ResultErrCap, "result.err", Some(1), |args: &[Value]| {
    Ok(Some(make(false, args[0].clone())))
});

std_cap!(
    ResultIsOkCap,
    "result.is_ok",
    Some(1),
    |args: &[Value]| {
        let (ok, _) = open("result.is_ok", &args[0])?;
        Ok(Some(Value::Bool(ok)))
    }
);

std_cap!(
    ResultUnwrapCap,
    "result.unwrap",
    Some(1),
    |args: &[Value]| {
        match open("result.unwrap", &args[0])? {
            (true, value) => Ok(Some(value)),
            (false, err) => Err(format!("result.unwrap: called on err: {err}")),
        }
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        let mut caps = HostCaps::new();
        register(&mut caps);
        caps.get(name).expect(name).call(args)
    }

    #[test]
    fn ok_unwraps_and_err_does_not() {
        let ok = call("result.ok", vec![Value::Int(7)]).unwrap().unwrap();
        let err = call("result.err", vec![Value::Str("boom".into())])
            .unwrap()
            .unwrap();
        assert_eq!(
            call("result.is_ok", vec![ok.clone()]),
            Ok(Some(Value::Bool(true)))
        );
        assert_eq!(
            call("result.is_ok", vec![err.clone()]),
            Ok(Some(Value::Bool(false)))
        );
        assert_eq!(call("result.unwrap", vec![ok]), Ok(Some(Value::Int(7))));
        let msg = call("result.unwrap", vec![err]).unwrap_err();
        assert!(msg.contains("called on err: boom"), "{msg}");
    }

    #[test]
    fn arbitrary_maps_are_not_results() {
        let mut m = HashMap::new();
        m.insert("ok".to_string(), Value::Bool(true));
        m.insert("value".to_string(), Value::Int(1));
        m.insert("extra".to_string(), Value::Null);
        for v in [Value::new_map(m), Value::Int(1), Value::Null] {
            assert!(
                call("result.is_ok", vec![v])
                    .unwrap_err()
                    .contains("expected a result")
            );
        }
    }
}
