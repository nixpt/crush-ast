//! CRUSH-xps-13: source-located method contract diagnostics.
//! Keep SDK capability contracts intact; typed adapters and shared kernels
//! are separate slices. Ownership adapters remain checked by `values`.
use serde_json::Value;
use std::collections::HashMap;

type Meta = HashMap<String, Value>;

pub(crate) fn error(meta: &Meta, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "unsupported Rust method at {}:{}: {reason} (CRUSH-xps-13)",
        meta.get("line").unwrap_or(&Value::Null),
        meta.get("column").unwrap_or(&Value::Null),
    )
}
