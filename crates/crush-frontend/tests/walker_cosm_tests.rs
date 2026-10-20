//! CRUSH-277 S1: a walker-supplied COSM module (CAST `Program::cosm`) is
//! merged into the program's kind table and reaches CASM — while strategies
//! no engine runs yet stay loud at compile time.

use crush_cast::{Function, Program};
use crush_cosm::{
    Construct, CosmModule, Equality, FnRef, Hooks, KindDecl, Receiver, Resolution, Semantics,
    Storage,
};
use std::collections::HashMap;

fn program_with_cosm(cosm: Option<CosmModule>) -> Program {
    let mut functions = HashMap::new();
    functions.insert(
        "main".to_string(),
        Function {
            params: vec![],
            body: vec![],
            meta: HashMap::new(),
            is_async: false,
            annotations: None,
        },
    );
    Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("javascript".to_string()),
        functions,
        cosm,
        ..Default::default()
    }
}

fn kind(name: &str, resolution: Resolution) -> KindDecl {
    KindDecl {
        name: name.to_string(),
        home_lang: "javascript".to_string(),
        span: None,
        storage: Storage::OpenDict,
        semantics: Semantics::Reference,
        parents: vec![],
        resolution,
        receiver: Receiver::DynamicThis,
        construct: Construct::JsNew {
            ctor_fn: FnRef::new(format!("{name}::constructor")),
        },
        members: vec![],
        hooks: Hooks::default(),
        conforms: vec![],
        equality: Equality::Identity,
    }
}

fn module(kinds: Vec<KindDecl>) -> CosmModule {
    CosmModule {
        kinds,
        interfaces: vec![],
    }
}

#[test]
fn walker_module_reaches_casm_kind_table() {
    // Resolution::Single is a strategy the COSM engines already run; the
    // transport must not care which walker produced the module.
    let program = program_with_cosm(Some(module(vec![kind("Counter", Resolution::Single)])));
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.expect("module survives compilation");
    assert_eq!(cosm.kinds.len(), 1);
    assert_eq!(cosm.kinds[0].name, "Counter");
    let table = casm.cosm.expect("CASM carries the kind table");
    assert_eq!(table.kinds.len(), 1);
    assert_eq!(table.kinds[0].home_lang, "javascript");
}

#[test]
fn prototype_chain_compiles_and_reaches_casm() {
    // CRUSH-277 S2: the scheduler/portable VM gate runs prototype chains,
    // so a JS kind compiles through the frontend like any other.
    let program = program_with_cosm(Some(module(vec![kind(
        "Counter",
        Resolution::PrototypeChain,
    )])));
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    assert_eq!(cosm.expect("module survives").kinds[0].name, "Counter");
    assert_eq!(casm.cosm.expect("kind table carried").kinds.len(), 1);
}

#[test]
fn embedding_promotion_still_stays_loud() {
    // The refusal mechanism itself is unchanged: a strategy no engine
    // runs (Go's, K3) must fail compilation naming it, never silently
    // run as a map.
    let program = program_with_cosm(Some(module(vec![kind(
        "Counter",
        Resolution::EmbeddingPromotion,
    )])));
    let err = match crush_frontend::compile_cast_with_cosm(program) {
        Ok(_) => panic!("embedding_promotion compiled before the Go walker"),
        Err(e) => format!("{e:#}"),
    };
    assert!(
        err.contains("embedding_promotion"),
        "refusal names the strategy: {err}"
    );
}

#[test]
fn empty_walker_module_counts_as_absent() {
    let (casm, cosm) =
        crush_frontend::compile_cast_with_cosm(program_with_cosm(Some(module(vec![])))).unwrap();
    assert_eq!(cosm, None);
    assert_eq!(casm.cosm, None);
}

#[test]
fn walker_and_surface_declaration_of_one_name_collide() {
    let walker = module(vec![kind("Point", Resolution::Single)]);
    let derived = module(vec![kind("Point", Resolution::Single)]);
    let err = match crush_frontend::cosm_lower::merge_walker_module(Some(walker), Some(derived)) {
        Ok(_) => panic!("duplicate kind merged silently"),
        Err(e) => format!("{e:#}"),
    };
    assert!(err.contains("`Point`"), "collision names the kind: {err}");
    assert!(err.contains("both"), "collision explains itself: {err}");
}

#[test]
fn walker_module_with_an_unresolvable_parent_fails_validation() {
    let mut bad = kind("Child", Resolution::Single);
    bad.parents = vec![crush_cosm::KindRef::new("Missing")];
    let err = match crush_frontend::cosm_lower::merge_walker_module(Some(module(vec![bad])), None) {
        Ok(_) => panic!("unresolvable parent accepted"),
        Err(e) => format!("{e:#}"),
    };
    assert!(
        err.contains("Missing"),
        "validation names the parent: {err}"
    );
}
