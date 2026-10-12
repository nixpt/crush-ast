//! Shared map-key helpers for Index / arr_get / arr_set (CRUSH-200 review).
//!
//! Crush maps are string-keyed. Non-string keys are rejected rather than
//! stringified (so `d[1]` and `d["1"]` cannot collide). Missing keys are an
//! error, matching Python's KeyError posture rather than silently returning
//! null.

use crate::vm::{Value, VmError};
use std::collections::HashMap;

pub fn map_key_string(v: &Value) -> Result<String, VmError> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        other => Err(VmError::TypeError {
            expected: "string key",
            got: other.type_name(),
        }),
    }
}

pub fn map_get(map: &HashMap<String, Value>, key: &str) -> Result<Value, VmError> {
    map.get(key).cloned().ok_or(VmError::TypeError {
        expected: "present map key",
        got: "absent key",
    })
}
