//! `codebase.*` host capabilities — Step 5 of the AI-native roadmap.
//!
//! These caps expose the `crush-index` query API to Crush programs running
//! inside the CVM1.  A host that wants codebase navigation injects a
//! pre-built `CrushIndex` via `HostCapsBuilder::codebase(index)` before
//! running any programs.
//!
//! ## Available caps
//!
//! ```text
//! codebase.modules()                  → [{name, purpose, exports, related}]
//! codebase.definition("fn_name")      → {name, module, params, errors, reads, writes}
//! codebase.callers("fn_name")         → [{callee, caller_module, caller_fn, arg_count}]
//! codebase.invariants("module")       → [{name, description, applies_to, consequence}]
//! codebase.exhaustive_sites("Type")   → [{fn_name, covered_arms, file, line}]
//! codebase.uncovered_paths()          → [{fn_name, error_variant}]
//! ```
//!
//! All caps return `Value::Array` of `Value::Map` records.  Unknown / missing
//! values are `Value::Null`; lists are `Value::Array` of `Value::Str`.

use chrono::{NaiveDate, Utc};
use crush_cast::manifest::WeightedError;
use crush_index::stale::TempStaleChecker;
use crush_index::CrushIndex;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::collections::HashMap;
use std::sync::Arc;

/// Register all `codebase.*` capabilities into `caps`, snapping a
/// single shared `today = Utc::now().date_naive()` so the
/// `codebase.temporaries()` (unfiltered) and
/// `codebase.stale_temporaries()` (filtered) caps observe exactly the
/// same "now" — an agent reading `is_stale: true` from one and verifying
/// with the other can rely on a single wall-clock value, not
/// `temporaries_through_microsecond_drift`. Production callers prefer
/// this entrypoint; tests / specialised hosts that need a fixed
/// `today` use [`register_at`] directly so the canary tests don't
/// drift against wall-clock time.
pub fn register(caps: &mut HostCaps, index: Arc<CrushIndex>) {
    register_at(caps, index, Utc::now().date_naive());
}

/// Register all `codebase.*` capabilities, pinning the per-row age /
/// staleness predicates to the supplied `today`. Tests prefer this
/// entrypoint over [`register`] because `Utc::now()` would silently
/// rotate the boundary on each run, breaking canary assertions that
/// pin `today` to a hard-coded date (e.g. 2026-06-20). Specialised
/// hosts that want a frozen `today` for reproducibility can also use
/// this directly.
pub fn register_at(
    caps: &mut HostCaps,
    index: Arc<CrushIndex>,
    today: NaiveDate,
) {
    caps.register(Box::new(CodebaseModulesCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseDefinitionCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseCallersCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseInvariantsCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseExhaustiveSitesCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseUncoveredPathsCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseWipCap(Arc::clone(&index))));
    // Shared `today` for both temporaries caps — see [`register`]
    // doc for why this matters.
    caps.register(Box::new(CodebaseTemporariesCap::new(
        Arc::clone(&index),
        today,
    )));
    caps.register(Box::new(CodebaseDecisionsCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseStaleTemporariesCap::new(
        Arc::clone(&index),
        today,
    )));
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn str_list(items: &[String]) -> Value {
    Value::new_array(items.iter().map(|s| Value::Str(s.clone())).collect())
}

fn make_map(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let m: HashMap<String, Value> = pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    Value::new_map(m)
}

/// Serialise probabilistic error annotations for `codebase.definition`.
///
/// Each entry is a 2-field map carrying the error variant and its
/// likelihood tag (e.g. `"Foo"` / `"likely"`) as strings — not as
/// enum ordinals — so JSON consumers and agents reading the response
/// don't have to know the internal `ErrorLikelihood` ordering to
/// interpret the values. The descriptive tag travels with the variant.
///
/// Companion to the existing flat `errors: [...]` field on
/// `CodebaseDefinitionCap`'s response — the same `make_map` call now
/// emits an `errors_weighted: [...]` entry that previously went
/// missing (the `FunctionAnnotations.errors_weighted` field was added
/// in `crush-cast`, populated by `parse_weighted_errors`, but never
/// plumbed through to the cap response).
fn weighted_errors_list(items: &[WeightedError]) -> Value {
    let rows: Vec<Value> = items
        .iter()
        .map(|w| {
            make_map([
                ("variant", Value::Str(w.variant.clone())),
                (
                    "likelihood",
                    Value::Str(w.likelihood.to_string()),
                ),
            ])
        })
        .collect();
    Value::new_array(rows)
}

// ── codebase.modules() ───────────────────────────────────────────────────────

struct CodebaseModulesCap(Arc<CrushIndex>);

impl HostCap for CodebaseModulesCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "codebase.modules".to_string(), argc: Some(0), returns: true }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let rows: Vec<Value> = self
            .0
            .modules()
            .into_iter()
            .map(|m| {
                make_map([
                    ("name", Value::Str(m.module_path.clone())),
                    ("purpose", Value::Str(m.purpose.clone())),
                    ("exports", str_list(&m.exports)),
                    ("related", str_list(&m.related)),
                    (
                        "exhaustive_types",
                        str_list(&m.exhaustive_types),
                    ),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.definition("fn_name") ───────────────────────────────────────────

struct CodebaseDefinitionCap(Arc<CrushIndex>);

impl HostCap for CodebaseDefinitionCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "codebase.definition".to_string(), argc: Some(1), returns: true }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let name = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err("codebase.definition: arg must be a string".to_string()),
        };

        let Some(entry) = self.0.definition(&name) else {
            return Ok(Some(Value::Null));
        };

        let params: Vec<Value> = entry
            .params
            .iter()
            .map(|(n, t)| Value::Str(format!("{n}: {t}")))
            .collect();

        let (errors, reads, writes, does_not_write, covers, relies_on, errors_weighted) =
            if let Some(ann) = &entry.annotations {
                (
                    str_list(&ann.errors),
                    str_list(&ann.reads),
                    str_list(&ann.writes),
                    str_list(&ann.does_not_write),
                    str_list(&ann.covers),
                    str_list(&ann.relies_on),
                    weighted_errors_list(&ann.errors_weighted),
                )
            } else {
                let empty = Value::new_array(Vec::new());
                (
                    empty.clone(),
                    empty.clone(),
                    empty.clone(),
                    empty.clone(),
                    empty.clone(),
                    empty.clone(),
                    empty,
                )
            };

        Ok(Some(make_map([
            ("name", Value::Str(entry.name.clone())),
            ("module", Value::Str(entry.module_path.clone())),
            ("params", Value::new_array(params)),
            ("errors", errors),
            ("errors_weighted", errors_weighted),
            ("reads", reads),
            ("writes", writes),
            ("does_not_write", does_not_write),
            ("covers", covers),
            ("relies_on", relies_on),
        ])))
    }
}

// ── codebase.callers("fn_name") ──────────────────────────────────────────────

struct CodebaseCallersCap(Arc<CrushIndex>);

impl HostCap for CodebaseCallersCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "codebase.callers".to_string(), argc: Some(1), returns: true }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let name = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err("codebase.callers: arg must be a string".to_string()),
        };

        let rows: Vec<Value> = self
            .0
            .callers(&name)
            .into_iter()
            .map(|site| {
                make_map([
                    ("callee", Value::Str(site.callee.clone())),
                    ("caller_module", Value::Str(site.caller_module.clone())),
                    ("caller_fn", Value::Str(site.caller_fn.clone())),
                    ("arg_count", Value::Int(site.arg_count as i64)),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.invariants("module") ────────────────────────────────────────────

struct CodebaseInvariantsCap(Arc<CrushIndex>);

impl HostCap for CodebaseInvariantsCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "codebase.invariants".to_string(), argc: Some(1), returns: true }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let module = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err("codebase.invariants: arg must be a string".to_string()),
        };

        let rows: Vec<Value> = self
            .0
            .invariants(&module)
            .into_iter()
            .map(|inv| {
                make_map([
                    ("name", Value::Str(inv.name.clone())),
                    ("description", Value::Str(inv.description.clone())),
                    ("applies_to", str_list(&inv.applies_to)),
                    (
                        "consequence",
                        inv.consequence
                            .as_deref()
                            .map(|s| Value::Str(s.to_string()))
                            .unwrap_or(Value::Null),
                    ),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.exhaustive_sites("TypeName") ────────────────────────────────────

struct CodebaseExhaustiveSitesCap(Arc<CrushIndex>);

impl HostCap for CodebaseExhaustiveSitesCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "codebase.exhaustive_sites".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let type_name = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err("codebase.exhaustive_sites: arg must be a string".to_string()),
        };

        let rows: Vec<Value> = self
            .0
            .exhaustive_sites(&type_name)
            .into_iter()
            .map(|site| {
                make_map([
                    ("function_name", Value::Str(site.function_name.clone())),
                    ("type_name", Value::Str(site.type_name.clone())),
                    ("covered_arms", str_list(&site.covered_arms)),
                    ("missing_arms", str_list(&site.missing_arms)),
                    ("file", Value::Str(site.location.file.clone())),
                    ("line", Value::Int(site.location.line as i64)),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.uncovered_paths() ───────────────────────────────────────────────

struct CodebaseUncoveredPathsCap(Arc<CrushIndex>);

impl HostCap for CodebaseUncoveredPathsCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "codebase.uncovered_paths".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let rows: Vec<Value> = self
            .0
            .uncovered_paths()
            .into_iter()
            .map(|gap| {
                make_map([
                    ("fn_name", Value::Str(gap.fn_name.clone())),
                    ("error_variant", Value::Str(gap.error_variant.clone())),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.wip() ───────────────────────────────────────────────────────────

struct CodebaseWipCap(Arc<CrushIndex>);

impl HostCap for CodebaseWipCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec { name: "codebase.wip".to_string(), argc: Some(0), returns: true }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let Some(wip) = self.0.wip() else {
            return Ok(Some(Value::Null));
        };
        Ok(Some(make_map([
            ("intent", Value::Str(wip.intent.clone())),
            (
                "started_by",
                wip.started_by
                    .as_deref()
                    .map(|s| Value::Str(s.to_string()))
                    .unwrap_or(Value::Null),
            ),
            ("done", str_list(&wip.done)),
            ("todo", str_list(&wip.todo)),
            ("unresolved", str_list(&wip.unresolved)),
        ])))
    }
}

// ── codebase.temporaries() ───────────────────────────────────────────────────

/// Unfiltered view over every `@temporary` node, plus per-row age /
/// staleness metadata so consumers don't have to call
/// `codebase.temporaries()` AND `codebase.stale_temporaries()` and
/// join the results themselves. `today` is injected at construction
/// (same pattern as `CodebaseStaleTemporariesCap`); production hosts
/// default it to `Utc::now().date_naive()` via [`register`], tests
/// pin a deterministic date.
struct CodebaseTemporariesCap(Arc<CrushIndex>, NaiveDate);

impl CodebaseTemporariesCap {
    /// Build an unfiltered-temporaries cap whose `is_stale` /
    /// `days_old` predicates are pinned to `today`. `register()`
    /// defaults `today` to `Utc::now().date_naive()`; the constructor
    /// is `pub` so specialised hosts / tests can pin a fixed date.
    pub fn new(index: Arc<CrushIndex>, today: NaiveDate) -> Self {
        Self(index, today)
    }
}

impl HostCap for CodebaseTemporariesCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "codebase.temporaries".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let checker = TempStaleChecker::new(self.1);
        let rows: Vec<Value> = self
            .0
            .temporaries()
            .into_iter()
            .map(|tmp| {
                let days_old = checker
                    .days_old(tmp)
                    .map(Value::Int)
                    // `added` missing/non-ISO ⇒ `None` from the
                    // checker ⇒ `Value::Null` in the response. The
                    // field is always present; the tag IS the
                    // "unknown" marker.
                    .unwrap_or(Value::Null);
                make_map([
                    ("reason", Value::Str(tmp.reason.clone())),
                    (
                        "expires_when",
                        tmp.expires_when
                            .clone()
                            .map(Value::Str)
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "owner",
                        tmp.owner.clone().map(Value::Str).unwrap_or(Value::Null),
                    ),
                    (
                        "added",
                        tmp.added.clone().map(Value::Str).unwrap_or(Value::Null),
                    ),
                    // `is_stale` defaults to `false` for unknown
                    // `added` (same silent-skip policy as the
                    // `stale_temporaries` cap), so consumers can trust
                    // `is_stale: false` ⇒ "either not old enough OR
                    // we don't know".
                    ("is_stale", Value::Bool(checker.is_stale(tmp))),
                    ("days_old", days_old),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.stale_temporaries() ─────────────────────────────────────────────

// `STALE_DAYS` and `TempStaleChecker` now live in `crush_index::stale` (see
// the imports at the top of this file). That crate is the single source of
// truth shared with `crush_frontend::wip_check` — this cap and the compiler
// warning cannot drift out of sync because they reference the same numeric
// literal and the same predicate struct.

struct CodebaseStaleTemporariesCap(Arc<CrushIndex>, NaiveDate);

impl CodebaseStaleTemporariesCap {
    /// Build a cap whose stale-check predicate is pinned to `today`.
    ///
    /// `register()` defaults `today` to `Utc::now().date_naive()`; tests
    /// / offline passes use this constructor so the boundary can be
    /// exercised deterministically (see canary tests in `mod tests`).
    pub fn new(index: Arc<CrushIndex>, today: NaiveDate) -> Self {
        Self(index, today)
    }
}

impl HostCap for CodebaseStaleTemporariesCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "codebase.stale_temporaries".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let checker = TempStaleChecker::new(self.1);
        let rows: Vec<Value> = self
            .0
            .temporaries()
            .into_iter()
            .filter(|tmp| checker.is_stale(tmp))
            .map(|tmp| {
                make_map([
                    // `reason` is `String` (mandatory), not
                    // `Option<String>` — wrap it directly. The three
                    // OPTIONAL fields below use the `Option::map /
                    // unwrap_or(Null)` pattern.
                    ("reason", Value::Str(tmp.reason.clone())),
                    (
                        "expires_when",
                        tmp.expires_when
                            .clone()
                            .map(Value::Str)
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "owner",
                        tmp.owner.clone().map(Value::Str).unwrap_or(Value::Null),
                    ),
                    (
                        "added",
                        tmp.added.clone().map(Value::Str).unwrap_or(Value::Null),
                    ),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

// ── codebase.decisions() ────────────────────────────────────────────────────

struct CodebaseDecisionsCap(Arc<CrushIndex>);

impl HostCap for CodebaseDecisionsCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "codebase.decisions".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let rows: Vec<Value> = self
            .0
            .decisions()
            .into_iter()
            .map(|dec| {
                make_map([
                    ("name", Value::Str(dec.name.clone())),
                    ("chose", Value::Str(dec.chose.clone())),
                    ("over", str_list(&dec.over)),
                    ("because", Value::Str(dec.because.clone())),
                    ("revisit_if", str_list(&dec.revisit_if)),
                ])
            })
            .collect();
        Ok(Some(Value::new_array(rows)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-only imports — these names are referenced exclusively by
    // helpers inside `mod tests` (e.g. `stale_added()` → `Duration`,
    // `index_with_temporary(...)` → `TemporaryNode`, `boundary_added()`
    // → `STALE_DAYS`). Keeping them here instead of the file-level
    // import block avoids 3 `unused_imports` warnings on the lib build.
    use chrono::Duration;
    use crush_cast::manifest::TemporaryNode;
    use crush_index::stale::STALE_DAYS;
    use crush_cast::manifest::{
        ExhaustiveMatchSite, FunctionAnnotations, Invariant, ModuleManifest, SourceLoc,
    };
    use crush_cast::{Function, Program};

    fn make_index() -> CrushIndex {
        let mut index = CrushIndex::new();

        let mut prog = Program {
            cast_version: "1".to_string(),
            entry: "main".to_string(),
            manifest: Some(ModuleManifest {
                purpose: "test module".to_string(),
                exports: vec!["do_thing".to_string()],
                invariants: vec![Invariant {
                    name: "inv-1".to_string(),
                    description: "must be true".to_string(),
                    applies_to: vec!["do_thing".to_string()],
                    consequence: Some("boom".to_string()),
                    check_source: None,
                }],
                related: vec!["other".to_string()],
                exhaustive_types: vec!["MyEnum".to_string()],
                changelog: vec![],
            }),
            exhaustive_sites: vec![ExhaustiveMatchSite {
                type_name: "MyEnum".to_string(),
                function_name: "do_thing".to_string(),
                location: SourceLoc { file: "mod.crush".to_string(), line: 10, col: 0 },
                covered_arms: vec!["A".to_string(), "B".to_string()],
                missing_arms: vec![],
                has_wildcard: false,
            }],
            ..Default::default()
        };
        prog.functions.insert(
            "do_thing".to_string(),
            Function {
                annotations: Some(FunctionAnnotations {
                    errors: vec!["NotFound".to_string()],
                    reads: vec!["db".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        prog.functions.insert(
            "test_do_thing".to_string(),
            Function {
                annotations: Some(FunctionAnnotations {
                    covers: vec!["NotFound".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        index.add_program("mymod", &prog);
        index
    }

    fn first_array(v: Option<Value>) -> Vec<Value> {
        match v.unwrap() {
            Value::Array(a) => a.borrow().clone(),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    fn map_str(v: &Value, key: &str) -> String {
        match v {
            Value::Map(m) => match m.borrow().get(key) {
                Some(Value::Str(s)) => s.clone(),
                other => format!("{other:?}"),
            },
            other => panic!("expected Map, got {other:?}"),
        }
    }

    /// Read a `bool`-valued entry from a row Map. Panics on `Null` or
    /// any other type so a regression that flips `is_stale` to `Null`
    /// surfaces at the test boundary instead of silently passing.
    fn map_bool(v: &Value, key: &str) -> bool {
        match v {
            Value::Map(m) => match m.borrow().get(key) {
                Some(Value::Bool(b)) => *b,
                other => panic!("expected Bool for key {key}, got {other:?}"),
            },
            other => panic!("expected Map, got {other:?}"),
        }
    }

    /// Read an `i64`-valued entry, accepting `Null` as the "unknown"
    /// sentinel for the `days_old` field (when `added` is missing or
    /// non-ISO). Returns `Some(n)` for `Value::Int(n)` and `None`
    /// for `Value::Null`. Panics on any other type.
    fn map_int_or_null(v: &Value, key: &str) -> Option<i64> {
        match v {
            Value::Map(m) => match m.borrow().get(key) {
                Some(Value::Int(n)) => Some(*n),
                Some(Value::Null) => None,
                other => panic!("expected Int or Null for key {key}, got {other:?}"),
            },
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn modules_cap_returns_module_list() {
        let cap = CodebaseModulesCap(Arc::new(make_index()));
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(map_str(&rows[0], "name"), "mymod");
        assert_eq!(map_str(&rows[0], "purpose"), "test module");
    }

    #[test]
    fn definition_cap_returns_function_entry() {
        let cap = CodebaseDefinitionCap(Arc::new(make_index()));
        let result = cap.call(vec![Value::Str("do_thing".to_string())]).unwrap();
        assert_eq!(map_str(&result.unwrap(), "name"), "do_thing");
    }

    #[test]
    fn definition_cap_returns_null_for_unknown() {
        let cap = CodebaseDefinitionCap(Arc::new(make_index()));
        let result = cap.call(vec![Value::Str("nope".to_string())]).unwrap();
        assert!(matches!(result, Some(Value::Null)));
    }

    #[test]
    fn invariants_cap_returns_module_invariants() {
        let cap = CodebaseInvariantsCap(Arc::new(make_index()));
        let rows =
            first_array(cap.call(vec![Value::Str("mymod".to_string())]).unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(map_str(&rows[0], "name"), "inv-1");
    }

    #[test]
    fn exhaustive_sites_cap_returns_sites() {
        let cap = CodebaseExhaustiveSitesCap(Arc::new(make_index()));
        let rows =
            first_array(cap.call(vec![Value::Str("MyEnum".to_string())]).unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(map_str(&rows[0], "function_name"), "do_thing");
    }

    #[test]
    fn uncovered_paths_cap_with_cover_in_place() {
        let cap = CodebaseUncoveredPathsCap(Arc::new(make_index()));
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(rows.len(), 0, "NotFound is covered by test_do_thing");
    }

    #[test]
    fn uncovered_paths_cap_detects_gap() {
        let mut index = CrushIndex::new();
        let mut prog = Program {
            cast_version: "1".to_string(),
            entry: "f".to_string(),
            ..Default::default()
        };
        prog.functions.insert(
            "f".to_string(),
            Function {
                annotations: Some(FunctionAnnotations {
                    errors: vec!["MissingCase".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        index.add_program("m", &prog);

        let cap = CodebaseUncoveredPathsCap(Arc::new(index));
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(rows.len(), 1);
        assert_eq!(map_str(&rows[0], "error_variant"), "MissingCase");
    }

    /// Pin `today` for the canary tests so boundary math is reproducible
    /// without depending on wall-clock time.
    ///
    /// A real date in mid-2026 gives us:
    /// - `fresh`        = 30 days back from `TODAY`
    /// - `stale`        = 100 days back from `TODAY`
    /// - `boundary`     = exactly 90 days back from `TODAY` (`fresh`
    ///                    per the `AddedDate < threshold` semantics, not
    ///                    `<=`)
    fn pin_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 20).expect("hard-coded test date is valid")
    }

    fn fresh_added() -> String {
        pin_today().pred_opt().unwrap().to_string()
    }

    fn stale_added() -> String {
        (pin_today() - Duration::days(100)).to_string()
    }

    fn boundary_added() -> String {
        (pin_today() - Duration::days(STALE_DAYS)).to_string()
    }

    /// Build a 1-fn index with a single @temporary node carrying the
    /// given `added` string (and a marker reason so the test can find
    /// it without colliding with anything). The cap reads
    /// `Program::temporaries` (NOT `ModuleManifest::temporaries` — the
    /// manifest doesn't carry that field), so seed `prog.temporaries`
    /// directly. Going through the parser would couple the test to the
    /// parser's surface; seedy Program directly so the test stays stable
    /// even if the parser shifts.
    fn index_with_temporary(added: Option<&str>) -> CrushIndex {
        let mut prog = Program {
            cast_version: "1".to_string(),
            entry: "f".to_string(),
            temporaries: vec![TemporaryNode {
                reason: "canary".to_string(),
                expires_when: None,
                owner: None,
                added: added.map(|s| s.to_string()),
            }],
            ..Default::default()
        };
        prog.functions.insert(
            "f".to_string(),
            Function {
                ..Default::default()
            },
        );
        let mut index = CrushIndex::new();
        index.add_program("m", &prog);
        index
    }

    #[test]
    fn stale_temporaries_cap_returns_empty_when_no_temporaries() {
        let mut prog = Program {
            cast_version: "1".to_string(),
            entry: "f".to_string(),
            ..Default::default()
        };
        prog.functions.insert(
            "f".to_string(),
            Function {
                ..Default::default()
            },
        );
        let mut index = CrushIndex::new();
        index.add_program("m", &prog);

        let cap = CodebaseStaleTemporariesCap::new(Arc::new(index), pin_today());
        let rows = first_array(cap.call(vec![]).unwrap());
        assert!(
            rows.is_empty(),
            "expected empty array for programme with zero @temporary nodes, got {rows:?}"
        );
    }

    #[test]
    fn stale_temporaries_cap_filters_fresh_added_outside_window() {
        // 1 day back from today — well inside the 90-day window.
        let cap = CodebaseStaleTemporariesCap::new(
            Arc::new(index_with_temporary(Some(&fresh_added()))),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert!(
            rows.is_empty(),
            "added={} (1 day back) should be FRESH, but the cap emitted {:?}",
            fresh_added(),
            rows,
        );
    }

    #[test]
    fn stale_temporaries_cap_includes_stale_added() {
        // 100 days back from today — past the 90-day threshold.
        let cap = CodebaseStaleTemporariesCap::new(
            Arc::new(index_with_temporary(Some(&stale_added()))),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(
            rows.len(),
            1,
            "added={} (100 days back) should be STALE and included",
            stale_added(),
        );
        assert_eq!(map_str(&rows[0], "reason"), "canary");
        assert_eq!(map_str(&rows[0], "added"), stale_added());
    }

    #[test]
    fn stale_temporaries_cap_excludes_exactly_90_day_boundary() {
        // The boundary is FRESH (strict `<`). This test guards the
        // off-by-one between `<` and `<=` — if someone flips the
        // comparator during translation, this test breaks.
        let cap = CodebaseStaleTemporariesCap::new(
            Arc::new(index_with_temporary(Some(&boundary_added()))),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert!(
            rows.is_empty(),
            "added={} (exactly STALE_DAYS back) should be FRESH (strict <), got {:?}",
            boundary_added(),
            rows,
        );
    }

    #[test]
    fn stale_temporaries_cap_silently_skips_missing_added() {
        // `added: None` is not parseable, so per predicate semantics
        // (silent skip on missing) → fresh, not stale. Locks in the
        // spec-aligned "don't flag @temporary until `added` is set"
        // policy.
        let cap = CodebaseStaleTemporariesCap::new(
            Arc::new(index_with_temporary(None)),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert!(
            rows.is_empty(),
            "missing `added` should be silently skipped, not emitted"
        );
    }

    #[test]
    fn temporaries_cap_emits_is_stale_false_and_null_days_old_for_missing_added() {
        // Missing `added` survives in the unfiltered list (unlike
        // `stale_temporaries`, which silently drops it). `is_stale`
        // is `false` per the shared silent-skip policy; `days_old` is
        // `Null` because the checker can't compute an age from an
        // unparseable `added`.
        let cap = CodebaseTemporariesCap::new(
            Arc::new(index_with_temporary(None)),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(
            rows.len(),
            1,
            "missing-added row should still appear in the unfiltered list (only stale_temporaries drops it)"
        );
        assert_eq!(map_str(&rows[0], "reason"), "canary");
        assert!(
            !map_bool(&rows[0], "is_stale"),
            "missing 'added' should be silently treated as fresh (is_stale: false)"
        );
        assert_eq!(
            map_int_or_null(&rows[0], "days_old"),
            None,
            "missing 'added' should serialize days_old as Null (not 0, not -1)"
        );
    }

    #[test]
    fn temporaries_cap_emits_fresh_row_with_days_old_eq_1() {
        // 1-day-old row: not stale (strict <), days_old = 1.
        let cap = CodebaseTemporariesCap::new(
            Arc::new(index_with_temporary(Some(&fresh_added()))),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(rows.len(), 1);
        assert!(
            !map_bool(&rows[0], "is_stale"),
            "1-day-old row should NOT be stale (90-day window)"
        );
        assert_eq!(
            map_int_or_null(&rows[0], "days_old"),
            Some(1),
            "1 day back from today → days_old: 1"
        );
        assert_eq!(map_str(&rows[0], "added"), fresh_added());
    }

    #[test]
    fn temporaries_cap_emits_boundary_row_with_days_old_eq_90() {
        // Boundary = today - STALE_DAYS. Strict-`<` semantics ⇒
        // FRESH. This locks the off-by-one between `<` and `<=`
        // across both the `is_stale` AND the `days_old` fields.
        let cap = CodebaseTemporariesCap::new(
            Arc::new(index_with_temporary(Some(&boundary_added()))),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(
            rows.len(),
            1,
            "boundary row should appear in unfiltered list (only stale_temporaries drops it)"
        );
        assert!(
            !map_bool(&rows[0], "is_stale"),
            "exactly 90-day-old row should be FRESH (strict <: 90 not < 90)"
        );
        assert_eq!(
            map_int_or_null(&rows[0], "days_old"),
            Some(STALE_DAYS),
            "today - STALE_DAYS → days_old: 90"
        );
    }

    #[test]
    fn temporaries_cap_emits_stale_row_with_days_old_eq_100() {
        // 100-day-old row: stale (100 > 90 by strict <), days_old = 100.
        let cap = CodebaseTemporariesCap::new(
            Arc::new(index_with_temporary(Some(&stale_added()))),
            pin_today(),
        );
        let rows = first_array(cap.call(vec![]).unwrap());
        assert_eq!(rows.len(), 1);
        assert!(
            map_bool(&rows[0], "is_stale"),
            "100-day-old row should be STALE (100 strictly > 90)"
        );
        assert_eq!(
            map_int_or_null(&rows[0], "days_old"),
            Some(100),
            "100 days back from today → days_old: 100"
        );
        assert_eq!(map_str(&rows[0], "added"), stale_added());
    }

    #[test]
    fn builder_registers_all_codebase_caps() {
        use crate::host_caps::HostCapsBuilder;
        let caps = HostCapsBuilder::new().codebase(make_index()).build();
        for name in [
            "codebase.modules",
            "codebase.definition",
            "codebase.callers",
            "codebase.invariants",
            "codebase.exhaustive_sites",
            "codebase.uncovered_paths",
            "codebase.wip",
            "codebase.temporaries",
            "codebase.stale_temporaries",
        ] {
            assert!(caps.get(name).is_some(), "{name} not registered");
        }
    }
}
