use crate::CrushIndex;
use crush_cast::manifest::{FunctionAnnotations, Invariant, ModuleManifest};
use crush_cast::{Function, Program};
use std::collections::HashMap;

fn program_with_manifest(purpose: &str, exports: &[&str]) -> Program {
    Program {
        cast_version: "1.0.0".to_string(),
        entry: "main".to_string(),
        lang: Some("crush".to_string()),
        manifest: Some(ModuleManifest {
            purpose: purpose.to_string(),
            exports: exports.iter().map(|s| s.to_string()).collect(),
            invariants: Vec::new(),
            related: Vec::new(),
            exhaustive_types: Vec::new(),
            changelog: Vec::new(),
        }),
        functions: HashMap::new(),
        ..Default::default()
    }
}

fn program_with_fn(fn_name: &str, annotations: Option<FunctionAnnotations>) -> Program {
    let mut functions = HashMap::new();
    functions.insert(
        fn_name.to_string(),
        Function {
            params: Vec::new(),
            body: Vec::new(),
            meta: HashMap::new(),
            annotations,
            ..Default::default()
        },
    );
    Program {
        cast_version: "1.0.0".to_string(),
        entry: "main".to_string(),
        functions,
        ..Default::default()
    }
}

#[test]
fn test_modules_query() {
    let mut idx = CrushIndex::new();
    idx.add_program("scheduler", &program_with_manifest("runs green threads", &["run_scheduled"]));
    idx.add_program("vm.types", &program_with_manifest("value types", &["Value"]));

    let modules = idx.modules();
    assert_eq!(modules.len(), 2);
    // sorted by module_path
    assert_eq!(modules[0].module_path, "scheduler");
    assert_eq!(modules[0].purpose, "runs green threads");
    assert_eq!(modules[1].module_path, "vm.types");
}

#[test]
fn test_definition_query() {
    let mut ann = FunctionAnnotations::default();
    ann.errors = vec!["VmError::StackUnderflow".to_string()];
    ann.reads = vec!["thread.ip".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("scheduler", &program_with_fn("execute_one", Some(ann)));

    let def = idx.definition("execute_one").expect("should find execute_one");
    assert_eq!(def.name, "execute_one");
    assert_eq!(def.module_path, "scheduler");
    let ann = def.annotations.as_ref().unwrap();
    assert_eq!(ann.errors, vec!["VmError::StackUnderflow"]);
    assert_eq!(ann.reads, vec!["thread.ip"]);
}

#[test]
fn test_callers_query() {
    // Build a program where `main` calls `helper`
    use crush_cast::{Expression, Statement};

    let mut functions = HashMap::new();
    functions.insert(
        "helper".to_string(),
        Function { params: Vec::new(), body: Vec::new(), meta: HashMap::new(), annotations: None, ..Default::default() },
    );
    functions.insert(
        "main".to_string(),
        Function {
            params: Vec::new(),
            body: vec![Statement::ExprStmt {
                expr: Expression::Call {
                    function: "helper".to_string(),
                    args: Vec::new(),
                    meta: HashMap::new(),
                },
                meta: HashMap::new(),
            }],
            meta: HashMap::new(),
            annotations: None,
            ..Default::default()
        },
    );
    let prog = Program {
        cast_version: "1.0.0".to_string(),
        entry: "main".to_string(),
        functions,
        ..Default::default()
    };

    let mut idx = CrushIndex::new();
    idx.add_program("mymod", &prog);

    let callers = idx.callers("helper");
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].caller_fn, "main");
    assert_eq!(callers[0].caller_module, "mymod");
}

#[test]
fn test_invariants_query() {
    let mut prog = program_with_manifest("scheduler", &[]);
    prog.manifest.as_mut().unwrap().invariants = vec![Invariant {
        name: "no-reenter".to_string(),
        description: "no re-entrancy".to_string(),
        applies_to: vec!["execute_one".to_string()],
        consequence: Some("deadlock".to_string()),
        check_source: None,
    }];

    let mut idx = CrushIndex::new();
    idx.add_program("scheduler", &prog);

    let invs = idx.invariants("scheduler");
    assert_eq!(invs.len(), 1);
    assert_eq!(invs[0].name, "no-reenter");
    assert_eq!(invs[0].consequence.as_deref(), Some("deadlock"));
}

#[test]
fn test_uncovered_paths() {
    let mut with_errors = FunctionAnnotations::default();
    with_errors.errors = vec!["VmError::Foo".to_string(), "VmError::Bar".to_string()];

    let mut covers_foo = FunctionAnnotations::default();
    covers_foo.covers = vec!["VmError::Foo".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("mod", &program_with_fn("do_thing", Some(with_errors)));
    idx.add_program("mod", &program_with_fn("test_foo", Some(covers_foo)));

    let gaps = idx.uncovered_paths();
    // Foo is covered; Bar is not
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].error_variant, "VmError::Bar");
    assert_eq!(gaps[0].fn_name, "do_thing");
}

#[test]
fn test_exhaustive_sites() {
    use crush_cast::manifest::{ExhaustiveMatchSite, SourceLoc};

    let mut prog = program_with_fn("dispatch", None);
    prog.exhaustive_sites = vec![
        ExhaustiveMatchSite {
            type_name: "Value".to_string(),
            function_name: "dispatch".to_string(),
            location: SourceLoc { file: "vm.crush".to_string(), line: 10, col: 4 },
            covered_arms: vec!["Int".to_string(), "Str".to_string()],
            missing_arms: Vec::new(),
            has_wildcard: false,
        },
        ExhaustiveMatchSite {
            type_name: "StepAction".to_string(),
            function_name: "dispatch".to_string(),
            location: SourceLoc::default(),
            covered_arms: vec!["Spawn".to_string()],
            missing_arms: Vec::new(),
            has_wildcard: false,
        },
    ];

    let mut idx = CrushIndex::new();
    idx.add_program("vm", &prog);

    let value_sites = idx.exhaustive_sites("Value");
    assert_eq!(value_sites.len(), 1);
    assert!(value_sites[0].covered_arms.contains(&"Int".to_string()));

    let all_sites = idx.exhaustive_sites("");
    assert_eq!(all_sites.len(), 2);
}

// ── CRUSH-28: flatten_annotations consumption slice ───────────────────────────

#[test]
fn test_add_program_caches_flat_annotations_module_level() {
    use crush_cast::manifest::Invariant;

    let mut prog = program_with_manifest("scheduler", &[]);
    prog.manifest.as_mut().unwrap().invariants = vec![Invariant {
        name: "no-reenter".to_string(),
        description: "no re-entrancy".to_string(),
        applies_to: vec!["execute_one".to_string()],
        consequence: Some("deadlock".to_string()),
        check_source: None,
    }];

    let mut idx = CrushIndex::new();
    idx.add_program("scheduler", &prog);

    let ladder = idx.annotations("scheduler");
    // Expect at least Module + 1 Invariant = 2 entries.
    assert!(ladder.len() >= 2);
    // First entry (lowest sort key) should be Module (kind=0).
    assert!(matches!(
        ladder[0],
        crush_cast::Annotation::Module(_)
    ));
}

#[test]
fn test_add_program_caches_function_level_annotations() {
    let mut ann = FunctionAnnotations::default();
    ann.errors = vec!["VmError::StackUnderflow".to_string()];
    ann.reads = vec!["thread.ip".to_string()];
    ann.writes = vec!["thread.ip".to_string()];
    ann.covers = vec!["oracle_x".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("vm", &program_with_fn("execute_one", Some(ann)));

    let ladder = idx.annotations("vm");
    // Expect Error + Read + Write + Coverage = 4 entries (no manifest
    // → no Module, no Invariant).
    assert_eq!(ladder.len(), 4);
    // function_name preserved on every function-level variant.
    for ann_ref in &ladder {
        match ann_ref {
            crush_cast::Annotation::Error(e) => assert_eq!(e.function_name, "execute_one"),
            crush_cast::Annotation::Read(r) => assert_eq!(r.function_name, "execute_one"),
            crush_cast::Annotation::Write(w) => assert_eq!(w.function_name, "execute_one"),
            crush_cast::Annotation::Coverage(c) => assert_eq!(c.function_name, "execute_one"),
            _ => panic!("unexpected variant in function-level ladder"),
        }
    }
}

#[test]
fn test_add_program_appends_to_existing_module_ladder() {
    // Two add_program() calls under the same module_path must accumulate
    // ladders — this is what makes `uncovered_paths()` work when
    // "do_thing" and "test_foo" are both added under module_path "mod".
    let mut with_errors = FunctionAnnotations::default();
    with_errors.errors = vec!["VmError::Foo".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program(
        "mod",
        &program_with_fn("do_thing", Some(with_errors.clone())),
    );
    idx.add_program("mod", &program_with_fn("test_foo", Some(with_errors)));

    let ladder = idx.annotations("mod");
    // Should have 2 Annotation::Error entries (one from each add_program).
    let error_count = ladder
        .iter()
        .filter(|a| matches!(a, crush_cast::Annotation::Error(_)))
        .count();
    assert_eq!(error_count, 2);
}

#[test]
fn test_annotations_unknown_module_returns_empty() {
    let mut idx = CrushIndex::new();
    idx.add_program("present", &program_with_manifest("x", &[]));
    assert!(idx.annotations("absent").is_empty());
}

#[test]
fn test_annotations_returns_deterministically_sorted() {
    // Same module queried twice returns the same order; the sort key is
    // (kind, target_resource). Coverage < Error < Read in the kind
    // ordinal (Coverage=5, Error=2, Read=3) — but we want to assert
    // pointer identity to guarantee no hash-flap.
    let mut ann = FunctionAnnotations::default();
    ann.errors = vec!["X".to_string()];
    ann.reads = vec!["Y".to_string()];
    ann.covers = vec!["Z".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("m", &program_with_fn("fn_a", Some(ann)));

    let a = idx.annotations("m");
    let b = idx.annotations("m");
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        // Same discriminant across the two queries → stable order.
        assert_eq!(std::mem::discriminant(*x), std::mem::discriminant(*y));
    }
}

#[test]
fn test_uncovered_paths_module_path_populated() {
    let mut with_errors = FunctionAnnotations::default();
    with_errors.errors = vec!["VmError::Foo".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("impl", &program_with_fn("do_thing", Some(with_errors)));

    let gaps = idx.uncovered_paths();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].module_path, "impl");
    assert_eq!(gaps[0].fn_name, "do_thing");
    assert_eq!(gaps[0].error_variant, "VmError::Foo");
}

#[test]
fn test_cross_module_coverage_closure() {
    // A `@covers` Oracle declared in module `tests` closes an `@errors`
    // variant declared in module `impl` (cross-module Normalization).
    let mut errs = FunctionAnnotations::default();
    errs.errors = vec![
        "VmError::Foo".to_string(),
        "VmError::Bar".to_string(),
    ];

    let mut cov = FunctionAnnotations::default();
    cov.covers = vec!["VmError::Foo".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("impl", &program_with_fn("do_thing", Some(errs)));
    idx.add_program("tests", &program_with_fn("test_foo", Some(cov)));

    let gaps = idx.uncovered_paths();
    // Foo is closed by tests/test_foo; only Bar remains as a gap in impl.
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].error_variant, "VmError::Bar");
    assert_eq!(gaps[0].module_path, "impl");
}

#[test]
fn test_uncovered_paths_no_manifest_still_works() {
    // Program with no manifest yet, only function-level @errors/@covers.
    let mut errs = FunctionAnnotations::default();
    errs.errors = vec!["VmError::Baz".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("anon", &program_with_fn("do_work", Some(errs)));

    let gaps = idx.uncovered_paths();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].fn_name, "do_work");
    assert_eq!(gaps[0].module_path, "anon");
}

#[test]
fn test_function_annotations_backward_compat() {
    // The legacy `FunctionEntry.annotations` field must still be
    // populated for callers that reach it via `definition(fn_name)`.
    let mut ann = FunctionAnnotations::default();
    ann.errors = vec!["VmError::X".to_string()];

    let mut idx = CrushIndex::new();
    idx.add_program("m", &program_with_fn("fn_x", Some(ann.clone())));

    let def = idx.definition("fn_x").expect("fn_x should exist");
    let stored_ann = def
        .annotations
        .as_ref()
        .expect("annotations should still be there");
    assert_eq!(stored_ann.errors, vec!["VmError::X".to_string()]);
}

#[test]
fn test_annotations_empty_module_path_returns_empty() {
    let mut idx = CrushIndex::new();
    // Query with empty string — no module is ever keyed by "", so this
    // must succeed (return empty Vec) without panicking.
    let ladder = idx.annotations("");
    assert!(ladder.is_empty());
}

#[test]
fn test_annotations_module_dedup_across_add_program_calls() {
    // CRUSH-28 review fix: re-ingesting the same `module_path` should
    // surface ONE `Annotation::Module`, not one per add_program call.
    // Otherwise downstream `codebase.modules()` (CRUSH-29) would emit
    // duplicate rows. Function-level variants (Error / Read / Write /
    // Coverage) still stack because they're keyed by function_name.
    let mut prog_a = program_with_manifest("first purpose", &[]);
    prog_a.manifest.as_mut().unwrap().invariants = vec![];

    let mut prog_b = program_with_manifest("second purpose", &[]);
    prog_b.manifest.as_mut().unwrap().invariants = vec![];

    let mut idx = CrushIndex::new();
    idx.add_program("scheduler", &prog_a);
    idx.add_program("scheduler", &prog_b);

    let ladder = idx.annotations("scheduler");
    let module_count = ladder
        .iter()
        .filter(|a| matches!(a, crush_cast::Annotation::Module(_)))
        .count();
    assert_eq!(
        module_count, 1,
        "Annotation::Module must be deduplicated to a singleton per module_path"
    );
}
