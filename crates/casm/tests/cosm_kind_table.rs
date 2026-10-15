//! COSM K1.3 (CRUSH-zorro-42): `casm::Program` carries the kind table
//! through CASM JSON and binary CASM, and CASM written before the field
//! existed still loads.

use casm::{Format, Function, Instruction, Manifest, Program};
use crush_cosm::CosmModule;
use std::collections::HashMap;

/// The Point/Shape fixture from crush-cosm (COSM K1.1).
const POINT_FIXTURE: &str = include_str!("../../crush-cosm/tests/fixtures/crush_point.cosm.json");

fn point_module() -> CosmModule {
    CosmModule::from_json(POINT_FIXTURE).expect("fixture parses")
}

fn program(cosm: Option<CosmModule>) -> Program {
    let mut p = Program::new();
    p.lang = Some("crush".to_string());
    p.manifest = Manifest { permissions: vec!["io.print".to_string()] };
    let push = Instruction {
        op: "push_int".to_string(),
        lang: None,
        meta: None,
        args: serde_json::json!({"value": 7}),
    };
    p.functions.insert(
        "main".to_string(),
        Function { params: vec![], locals: vec![], type_hints: None, body: vec![push] },
    );
    p.cosm = cosm;
    p
}

#[test]
fn the_kind_table_round_trips_through_casm_json() {
    let p = program(Some(point_module()));
    let json = p.serialize(Format::Json).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    // Embedded as the COSM module itself, readable in the CASM JSON.
    assert_eq!(v["cosm"]["kinds"][0]["name"], "Point");
    let back = Program::deserialize(&json, Format::Json).unwrap();
    assert_eq!(back.cosm, Some(point_module()));
}

/// Binary CASM. (Its functions don't survive a binary round-trip, kind
/// table or not — `Instruction`'s flattened args encode as a map the
/// positional decoder doesn't accept — so these programs have none.)
fn binary_program(cosm: Option<CosmModule>) -> Program {
    let mut p = program(cosm);
    p.functions.clear();
    p
}

#[test]
fn the_kind_table_round_trips_through_binary_casm() {
    let p = binary_program(Some(point_module()));
    let bin = p.serialize(Format::Binary).unwrap();
    let back = Program::deserialize(&bin, Format::Binary).unwrap();
    assert_eq!(back.cosm, Some(point_module()));
    assert_eq!(back.lang.as_deref(), Some("crush"));
    assert_eq!(back.manifest.permissions, p.manifest.permissions);
}

#[test]
fn casm_json_without_the_field_loads_and_is_written_as_before() {
    let old = r#"{"version":"1.0","functions":{},"manifest":{"permissions":[]},"lang":null}"#;
    let p = Program::deserialize(old.as_bytes(), Format::Json).unwrap();
    assert_eq!(p.cosm, None);
    // No kind table → no `cosm` key: byte-identical to what older writers produced.
    let json = String::from_utf8(p.serialize(Format::Json).unwrap()).unwrap();
    assert!(!json.contains("cosm"), "{json}");
}

/// Binary CASM as written before the field existed: a four-field array.
#[test]
fn old_binary_casm_still_loads() {
    #[derive(serde::Serialize)]
    struct OldProgram {
        version: String,
        functions: HashMap<String, Function>,
        manifest: Manifest,
        lang: Option<String>,
    }
    let new = binary_program(None);
    let old = OldProgram {
        version: new.version.clone(),
        functions: new.functions.clone(),
        manifest: new.manifest.clone(),
        lang: new.lang.clone(),
    };
    let mut bytes = b"#!/usr/bin/env crush run\n".to_vec();
    rmp_serde::encode::write(&mut bytes, &old).unwrap();
    let p = Program::deserialize(&bytes, Format::Binary).unwrap();
    assert_eq!(p.cosm, None);
    assert_eq!(p.lang.as_deref(), Some("crush"));
    // And a program without a kind table is still written byte for byte the same.
    assert_eq!(new.serialize(Format::Binary).unwrap(), bytes);
}

/// The one rule every engine applies: a Kind that behaves as a plain map runs;
/// one with a strategy a map can't honour is refused, naming the engine, the
/// Kind and the strategy.
#[test]
fn engines_refuse_only_kinds_maps_cant_represent() {
    assert_eq!(program(None).refuse_cosm("X"), Ok(()));
    assert_eq!(program(Some(CosmModule::default())).refuse_cosm("X"), Ok(()));
    assert!(program(Some(CosmModule::default())).kind_table().is_none());
    // Point/Shape uses Crush's defaults: it runs as maps everywhere.
    assert_eq!(program(Some(point_module())).refuse_cosm("X"), Ok(()));

    let mut m = point_module();
    m.kinds[0].semantics = crush_cosm::Semantics::Value(crush_cosm::ValueMode::Move);
    m.kinds[0].equality = crush_cosm::Equality::Identity;
    assert_eq!(
        program(Some(m)).refuse_cosm("the X backend").unwrap_err(),
        "COSM objects aren't supported on the X backend yet (CRUSH-zorro-43): \
         `Point` declares semantics: value(move), equality: identity; \
         without Kinds its instances would silently run as plain maps"
    );

    // Every strategy a map can't honour is named.
    let mut m = point_module();
    m.kinds[0].construct = crush_cosm::Construct::Ctor { func: "Point::new".into() };
    m.kinds[0].hooks.str = Some(crush_cosm::HookImpl::Fn("Point::show".into()));
    m.kinds[0].members.push(crush_cosm::MemberDecl::accessor("len", Some("Point::len".into()), None));
    if let crush_cosm::Storage::Sealed { fields } = &mut m.kinds[0].storage {
        fields[0].default = Some(crush_cosm::Const::Int(0));
    }
    let e = program(Some(m)).refuse_cosm("X").unwrap_err();
    for want in ["construct: ctor", "protocol hooks", "accessor `len`", "a default for field `x`"] {
        assert!(e.contains(want), "missing {want:?}: {e}");
    }
}
