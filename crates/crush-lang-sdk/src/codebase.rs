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

use crush_index::CrushIndex;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::collections::HashMap;
use std::sync::Arc;

/// Register all `codebase.*` capabilities into `caps`.
pub fn register(caps: &mut HostCaps, index: Arc<CrushIndex>) {
    caps.register(Box::new(CodebaseModulesCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseDefinitionCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseCallersCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseInvariantsCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseExhaustiveSitesCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseUncoveredPathsCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseWipCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseTemporariesCap(Arc::clone(&index))));
    caps.register(Box::new(CodebaseDecisionsCap(Arc::clone(&index))));
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

        let (errors, reads, writes, does_not_write, covers, relies_on) =
            if let Some(ann) = &entry.annotations {
                (
                    str_list(&ann.errors),
                    str_list(&ann.reads),
                    str_list(&ann.writes),
                    str_list(&ann.does_not_write),
                    str_list(&ann.covers),
                    str_list(&ann.relies_on),
                )
            } else {
                let empty = Value::new_array(Vec::new());
                (
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

struct CodebaseTemporariesCap(Arc<CrushIndex>);

impl HostCap for CodebaseTemporariesCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "codebase.temporaries".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let rows: Vec<Value> = self
            .0
            .temporaries()
            .into_iter()
            .map(|tmp| {
                make_map([
                    ("reason", Value::Str(tmp.reason.clone())),
                    (
                        "expires_when",
                        tmp.expires_when
                            .as_deref()
                            .map(|s| Value::Str(s.to_string()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "owner",
                        tmp.owner
                            .as_deref()
                            .map(|s| Value::Str(s.to_string()))
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "added",
                        tmp.added
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
        ] {
            assert!(caps.get(name).is_some(), "{name} not registered");
        }
    }
}
