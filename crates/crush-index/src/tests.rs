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
        Function { params: Vec::new(), body: Vec::new(), meta: HashMap::new(), annotations: None },
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
        },
        ExhaustiveMatchSite {
            type_name: "StepAction".to_string(),
            function_name: "dispatch".to_string(),
            location: SourceLoc::default(),
            covered_arms: vec!["Spawn".to_string()],
            missing_arms: Vec::new(),
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
