//! Shared library surface for the `xtask` package.
//!
//! Exposes the [`diag`] module — NDJSON helpers shared between the
//! audit-binary (`main.rs`) and the lint-dejavue binary
//! (`lint_dejavue.rs`) so both emissions follow the same seven-field
//! wire shape as `crush_lang_sdk::theme::JsonDiagnostic`.
//!
//! Today only `diag` lives here; future shared helpers (rg-invocation
//! shims, target-crate enumeration, etc.) co-locate alongside it.
//! Replace-once-and-import throughout both binaries to keep the
//! three-way-drift risk from the prior duplicate-helper setup from
//! re-emerging.

pub mod diag;
