//! Query result types returned by `CrushIndex`.

use serde::{Deserialize, Serialize};

/// A single call site — one place in the code where a function is called.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSite {
    /// The function being called.
    pub callee: String,
    /// Module of the calling function.
    pub caller_module: String,
    /// Name of the calling function.
    pub caller_fn: String,
    /// Number of arguments at this call site.
    pub arg_count: usize,
}

/// An error path from `@errors` that has no corresponding `@covers` test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGap {
    /// Function that declares this error.
    pub fn_name: String,
    /// The error variant that is not covered.
    pub error_variant: String,
    /// Module that declared the `@errors` annotation (CRUSH-28).
    ///
    /// Defaults to `""` for ingestion paths that predate module-aware
    /// coverage tracking; `#[serde(default)]` keeps the JSON stable
    /// across older snapshots that don't carry this field.
    #[serde(default)]
    pub module_path: String,
}

