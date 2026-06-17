//! Navigation-layer CAST nodes for AI-native Crush programs.
//!
//! These types represent the `@module`, `@invariant`, `@errors`, `@reads`,
//! `@writes`, and `@covers` annotations from the AI-native roadmap.
//!
//! **Design split** from `ai.rs` (execution layer):
//! - `ai.rs` — what the program *does* at runtime (goals, tool-chains, delegation)
//! - `manifest.rs` — what the program *is* structurally (purpose, contracts, coverage)
//!
//! The compiler populates these nodes. The `crush-index` crate consumes them to
//! build the queryable codebase index. `codebase.*` host caps expose that index
//! to Crush programs running as agents.

use serde::{Deserialize, Serialize};

/// Module-level navigation manifest — the `@module { ... }` annotation.
///
/// Every Crush source file should declare one. Advisorily enforced today;
/// `--strict-manifest` (planned) will make it a hard compiler error to omit.
///
/// Attached to `Program.manifest`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ModuleManifest {
    /// One-line description of what this module does and why it exists.
    /// Required. This is what `codebase.modules()` returns — it must fit in
    /// one context line and answer "should I read this file?"
    pub purpose: String,

    /// Public symbol names this module exports to callers.
    /// Agents use this to know what they can call without reading the source.
    #[serde(default)]
    pub exports: Vec<String>,

    /// Named invariants this module upholds. Agents read these before touching
    /// the module to know what must remain true after their change.
    #[serde(default)]
    pub invariants: Vec<Invariant>,

    /// Semantically related modules. Not just imports — conceptual coupling.
    /// E.g. `scheduler` lists `vm.types` as related because it uses Value/Frame
    /// even though vm.types doesn't import scheduler.
    #[serde(default)]
    pub related: Vec<String>,

    /// Sum types declared as requiring exhaustive match coverage tracking.
    /// The compiler records every site that matches on these types in
    /// `Program.exhaustive_sites`. Agents query `codebase.exhaustive_sites()`
    /// before adding a new variant to know all sites that need updating.
    #[serde(default)]
    pub exhaustive_types: Vec<String>,

    /// Chronological change log (newest last). Lightweight dejavue integration —
    /// the compiler writes here from commit metadata when `--embed-changelog` is set.
    #[serde(default)]
    pub changelog: Vec<ChangelogEntry>,
}

/// A named, typed contract that must hold for the module to be correct.
///
/// `@invariant "name" { description: "...", applies_to: [...], consequence: "..." }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct Invariant {
    /// Machine-readable identifier. Kebab-case. E.g. `"rc-refcell-not-send"`.
    /// Used as a stable key in the index and in `@relies-on` references.
    pub name: String,

    /// Agent-readable description of what the invariant means.
    pub description: String,

    /// Function or type names this invariant constrains. An agent modifying
    /// any of these symbols should re-read the invariant first.
    #[serde(default)]
    pub applies_to: Vec<String>,

    /// What breaks if this invariant is violated. Helps agents understand
    /// the consequence of a change without needing to trace the full call graph.
    #[serde(default)]
    pub consequence: Option<String>,
}

/// A lightweight changelog entry. Date is ISO 8601 string (YYYY-MM-DD).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ChangelogEntry {
    /// ISO 8601 date. E.g. `"2026-06-17"`.
    pub date: String,
    /// What changed and why — the commit message essence.
    pub summary: String,
}

/// Function-level semantic annotations.
///
/// Attached to `Function.annotations`. All fields are optional — partial
/// annotation is valid. Agents use whichever fields are present.
///
/// Source syntax (planned):
/// ```crush
/// fn execute_one(thread, ...)
///     @errors  [StackUnderflow, StepQuota, BadJump]
///     @reads   [thread.ip, thread.stack]
///     @writes  [thread.ip, thread.stack, thread.out_parts]
///     @no-write [program]
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FunctionAnnotations {
    /// Error variants this function may produce.
    /// E.g. `["VmError::StackUnderflow", "VmError::StepQuota"]`.
    /// Agents use this to know what error handling is required at call sites.
    #[serde(default)]
    pub errors: Vec<String>,

    /// State paths this function reads but does not own.
    /// Helps agents reason about what must be valid before calling this function.
    #[serde(default)]
    pub reads: Vec<String>,

    /// State paths this function may mutate.
    /// Agents check this before passing shared state to the function.
    #[serde(default)]
    pub writes: Vec<String>,

    /// State paths this function guarantees it does NOT write.
    /// Stronger contract than absence from `writes` — explicitly checked by
    /// the compiler (planned) and trusted by agents reasoning about const-ness.
    #[serde(default)]
    pub does_not_write: Vec<String>,

    /// Error paths, code paths, or behavioral variants this test function covers.
    /// Only meaningful when the function is a test (name starts with `test_`).
    /// `codebase.uncovered_paths()` returns all error paths with no `@covers` test.
    ///
    /// E.g. `["VmError::StackUnderflow", "VmError::DivByZero"]`
    #[serde(default)]
    pub covers: Vec<String>,

    /// Invariant names (from the module manifest) this function relies on.
    /// An agent changing this function should re-read the listed invariants.
    #[serde(default)]
    pub relies_on: Vec<String>,

    /// Complexity hint 0–100. Agents use this to decide whether to read
    /// the full body or request a summary. 0 = trivial, 100 = extremely complex.
    #[serde(default)]
    pub complexity: Option<u8>,
}

/// A site in the CAST where a sum type is matched exhaustively.
///
/// Populated by the **compiler** when a type appears in `manifest.exhaustive_types`.
/// Not written by source authors directly. Stored in `Program.exhaustive_sites`.
///
/// Agents query `codebase.exhaustive_sites("Value")` before adding a new variant
/// to know every match site that will need a new arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ExhaustiveMatchSite {
    /// The type being matched on. E.g. `"Value"`.
    pub type_name: String,

    /// The function containing this match expression.
    pub function_name: String,

    /// Source location of the match expression.
    pub location: SourceLoc,

    /// Variant arms present in this match.
    #[serde(default)]
    pub covered_arms: Vec<String>,

    /// Variant arms MISSING from this match (populated after type definition
    /// is finalised; empty until then).
    #[serde(default)]
    pub missing_arms: Vec<String>,
}

/// A source location used for diagnostics and index navigation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SourceLoc {
    /// Relative file path from the workspace root.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub col: u32,
}
