//! Helpers for converting [`crush_vm::vm::Value`] into serde-friendly
//! representations.
//!
//! **The runtime conversion path is now the canonical `impl serde::Serialize
//! for Value`** in `crush-vm::vm::Value` (single source of truth). Callers
//! use `serde_json::to_value(&v)` or `serde_json::to_string(&v)` directly;
//! the previous local helpers (`util::value_to_json`, `bus::crush_value_to_json`,
//! `db::crush_value_to_json`, `stdlib::value_to_json`) were duplicates of
//! the same match body and have been deleted in favor of the trait impl.
//!
//! This module is retained as a stub so any downstream crate that referenced
//! `crate::util::value_to_json` keeps a valid module path during the
//! transition window. New code should not add helpers here — extend
//! `impl serde::Serialize for Value` (or `impl serde::Deserialize for Value`)
//! instead.
