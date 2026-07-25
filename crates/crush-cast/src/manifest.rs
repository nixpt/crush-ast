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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

    /// Optional source expression that can be evaluated to check this invariant.
    /// Phase 2b will execute this; Phase 2a only stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_source: Option<String>,
}

/// A lightweight changelog entry. Date is ISO 8601 string (YYYY-MM-DD).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ChangelogEntry {
    /// ISO 8601 date. E.g. `"2026-06-17"`.
    pub date: String,
    /// What changed and why — the commit message essence.
    pub summary: String,
}

/// Likelihood of an error variant being produced by a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "lowercase")]
pub enum ErrorLikelihood {
    /// >50% of error cases.
    Likely,
    /// 5–50% of error cases.
    Possible,
    /// <5% of error cases.
    Rare,
}

impl std::fmt::Display for ErrorLikelihood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Likely => write!(f, "likely"),
            Self::Possible => write!(f, "possible"),
            Self::Rare => write!(f, "rare"),
        }
    }
}

/// An error variant annotated with a probabilistic likelihood.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WeightedError {
    /// Error variant name, e.g. `"NetworkTimeout"`.
    pub variant: String,
    /// Likelihood level.
    pub likelihood: ErrorLikelihood,
}

/// A `@wip` node declaring in-progress work on a module.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WipNode {
    /// One-line description of the in-progress task.
    pub intent: String,
    /// Agent or human who started this work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    /// Subtasks already completed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub done: Vec<String>,
    /// Subtasks still to be done.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todo: Vec<String>,
    /// Open questions blocking or complicating completion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<String>,
}

/// A `@temporary` node declaring technical debt with an intended expiry condition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TemporaryNode {
    /// Why this temporary code exists.
    pub reason: String,
    /// Condition under which it should be removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_when: Option<String>,
    /// Who is responsible for removing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// ISO 8601 date when this block was added, e.g. `"2026-06-17"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<String>,
}

/// A `@decision` node recording an architectural choice and its rationale.
///
/// Agents query `codebase.decisions()` before touching an unusual design to
/// understand why it was chosen over alternatives — and whether conditions
/// that should trigger a re-evaluation are now met.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct DecisionNode {
    /// Machine-readable name. Kebab-case. E.g. `"use-rc-refcell-not-arc-mutex"`.
    pub name: String,
    /// The option that was chosen.
    pub chose: String,
    /// Alternatives that were considered and rejected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub over: Vec<String>,
    /// Why this option was chosen.
    pub because: String,
    /// Conditions under which this decision should be revisited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisit_if: Vec<String>,
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

    /// Probabilistic error annotations from `@errors { Variant: likely }` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors_weighted: Vec<WeightedError>,

    /// State paths that this function invalidates after it returns.
    /// Callers must not hold references to these paths across the call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates: Vec<String>,

    /// Functions that MUST be called before this one at every call site.
    /// `E-MUT-001` is emitted when the ordering is violated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_call_before: Vec<String>,

    /// Functions that MUST be called after this one at every call site.
    /// `E-MUT-002` is emitted when the ordering is violated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_call_after: Vec<String>,
}

/// A site in the CAST where a sum type is matched exhaustively.
///
/// Populated by the **compiler** when a type appears in `manifest.exhaustive_types`.
/// Not written by source authors directly. Stored in `Program.exhaustive_sites`.
///
/// Agents query `codebase.exhaustive_sites("Value")` before adding a new variant
/// to know every match site that will need a new arm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// True when the match contains a wildcard arm (`_ => { ... }`).
    ///
    /// A wildcard silences the exhaustiveness check because it hides any number
    /// of unhandled variants.  `check_exhaustiveness()` emits `E-EXH-001` here.
    #[serde(default)]
    pub has_wildcard: bool,
}

/// A source location used for diagnostics and index navigation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

// ─── CRUSH-27: flat annotation ladder ─────────────────────────────────────
//
// The structural types above (ModuleManifest, Invariant, FunctionAnnotations,
// ExhaustiveMatchSite, DecisionNode, WipNode, TemporaryNode) stay as the AST
// storage; this section adds the flat `Annotation` enum that downstream
// consumers (CRUSH-28's `crush-index`, CRUSH-29's `codebase.*` host caps)
// iterate uniformly. Each variant carries the minimum context it needs to be
// useful (e.g. function-scoped Read/Write/Coverage variants bill
// `function_name` separately because the flat list loses ownership context).

/// Function-scoped `@errors` annotation — covers both list and weighted forms.
///
/// Sources:
/// - `@errors [VariantA, VariantB]` (`fn.errors` populated)
/// - `@errors { VariantA: likely, VariantB: rare }` (`fn.errors_weighted`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ErrorAnnotation {
    /// Function this annotation scopes to.
    pub function_name: String,
    /// Simple-list form entries (`@errors [a, b]`).
    #[serde(default)]
    pub variants: Vec<String>,
    /// Weighted-form entries (`@errors { a: likely }`). Empty for list form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants_weighted: Vec<WeightedError>,
}

/// Function-scoped `@reads [path]` annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReadAnnotation {
    pub function_name: String,
    pub paths: Vec<String>,
}

/// Function-scoped `@writes [path]` annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WriteAnnotation {
    pub function_name: String,
    pub paths: Vec<String>,
}

/// Function-scoped `@covers [path]` / `@covers "name"` annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct CoverageAnnotation {
    pub function_name: String,
    pub paths: Vec<String>,
}

/// Flat annotation ladder — single sequence downstream consumers iterate.
///
/// Seven variants, in ticket literal order. Re-exported as
/// `crush_cast::Annotation` so downstream consumers can `Vec<Annotation>`
/// uniformly regardless of whether the source annotation was module-level,
/// function-level, or compiler-populated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(tag = "kind", content = "node")]
pub enum Annotation {
    /// `@module { purpose, exports, invariants, related, exhaustive_types, changelog }`
    /// Reuses `ModuleManifest` as the payload (carries the full module-level
    /// surface; downstream `crush-index` reaches in for invariants/related/etc).
    Module(ModuleManifest),
    /// `@invariant "name" { description, applies_to, consequence, check_source }`
    /// Reuses `Invariant` as payload (one per `@invariant` block; can also
    /// appear inside `ModuleManifest.invariants` if that's where the source
    /// lives — `From<&Program>` dedupes by checking both roots).
    Invariant(Invariant),
    /// `@errors` scoped per function (see [`ErrorAnnotation`]).
    Error(ErrorAnnotation),
    /// `@reads` scoped per function (see [`ReadAnnotation`]).
    Read(ReadAnnotation),
    /// `@writes` scoped per function (see [`WriteAnnotation`]).
    Write(WriteAnnotation),
    /// `@covers` scoped per function (see [`CoverageAnnotation`]).
    Coverage(CoverageAnnotation),
    /// Compiler-populated match sites (from `Program.exhaustive_sites`).
    ExhaustiveMatchSites(ExhaustiveMatchSite),
}

impl crate::Program {
    /// Flatten this Program into a `Vec<Annotation>` that downstream
    /// consumers (e.g. `crush-index` in CRUSH-28) can iterate uniformly.
    ///
    /// Intentionally scoped to the **7 declared variants** of [`Annotation`]:
    /// `DecisionNode`, `WipNode`, `TemporaryNode`, and `ChangelogEntry` are
    /// NOT promoted into the flat ladder because they are human-coordination
    /// metadata, not data-level contract annotations, and conflating them
    /// would muddy the API.
    ///
    /// Emits BOTH `Annotation::Module` (if `Program.manifest` is set) AND
    /// one `Annotation::Invariant(inv)` per invariant in the module's
    /// `invariants` list — Module and per-invariant variants coexist in
    /// the flat ladder by design, so `crush-index` can answer queries like
    /// "show me this module's purpose" and "show me this one invariant"
    /// from the same iteration without a second pass.
    pub fn flatten_annotations(&self) -> Vec<Annotation> {
        let mut out: Vec<Annotation> = Vec::new();

        if let Some(manifest) = &self.manifest {
            out.push(Annotation::Module(manifest.clone()));
            for inv in &manifest.invariants {
                out.push(Annotation::Invariant(inv.clone()));
            }
        }

        for site in &self.exhaustive_sites {
            out.push(Annotation::ExhaustiveMatchSites(site.clone()));
        }

        for (name, func) in &self.functions {
            let Some(fa) = &func.annotations else { continue };
            if !fa.errors.is_empty() || !fa.errors_weighted.is_empty() {
                out.push(Annotation::Error(ErrorAnnotation {
                    function_name: name.clone(),
                    variants: fa.errors.clone(),
                    variants_weighted: fa.errors_weighted.clone(),
                }));
            }
            if !fa.reads.is_empty() {
                out.push(Annotation::Read(ReadAnnotation {
                    function_name: name.clone(),
                    paths: fa.reads.clone(),
                }));
            }
            if !fa.writes.is_empty() {
                out.push(Annotation::Write(WriteAnnotation {
                    function_name: name.clone(),
                    paths: fa.writes.clone(),
                }));
            }
            if !fa.covers.is_empty() {
                out.push(Annotation::Coverage(CoverageAnnotation {
                    function_name: name.clone(),
                    paths: fa.covers.clone(),
                }));
            }
        }

        out
    }
}
