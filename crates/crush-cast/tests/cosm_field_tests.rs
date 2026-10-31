//! CRUSH-277 S1: `Program::cosm` — a walker-supplied COSM module rides in
//! CAST beside the functions. Cosm-less programs must serialize exactly as
//! before the field existed.

use crush_cast::Program;
use crush_cosm::{
    Construct, CosmModule, Equality, FnRef, Hooks, KindDecl, Receiver, Resolution, Semantics,
    Storage,
};
use std::collections::HashMap;

fn plain_program() -> Program {
    Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("javascript".to_string()),
        functions: HashMap::new(),
        ..Default::default()
    }
}

fn js_kind() -> KindDecl {
    KindDecl {
        name: "Counter".to_string(),
        home_lang: "javascript".to_string(),
        span: None,
        storage: Storage::OpenDict,
        semantics: Semantics::Reference,
        parents: vec![],
        resolution: Resolution::PrototypeChain,
        receiver: Receiver::DynamicThis,
        construct: Construct::JsNew {
            ctor_fn: FnRef::new("Counter::constructor"),
        },
        members: vec![],
        hooks: Hooks::default(),
        conforms: vec![],
        equality: Equality::Identity,
    }
}

#[test]
fn cosm_less_program_serializes_without_the_field_and_round_trips() {
    let program = plain_program();
    let json = serde_json::to_string(&program).unwrap();
    assert!(
        !json.contains("\"cosm\""),
        "cosm leaked into cosm-less CAST: {json}"
    );
    let back: Program = serde_json::from_str(&json).unwrap();
    assert_eq!(back.cosm, None);
    assert_eq!(back.entry, "main");
}

#[test]
fn cast_written_before_the_field_deserializes_with_no_cosm() {
    let old = r#"{"cast_version":"0.2","entry":"main","lang":"javascript","functions":{}}"#;
    let program: Program = serde_json::from_str(old).unwrap();
    assert_eq!(program.cosm, None);
}

#[test]
fn program_with_cosm_round_trips() {
    let mut program = plain_program();
    program.cosm = Some(CosmModule {
        kinds: vec![js_kind()],
        interfaces: vec![],
    });
    let json = serde_json::to_string(&program).unwrap();
    assert!(
        json.contains("\"prototype_chain\""),
        "strategies missing: {json}"
    );
    let back: Program = serde_json::from_str(&json).unwrap();
    assert_eq!(back.cosm, program.cosm);
}
