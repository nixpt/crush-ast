//! The design doc's worked examples: build → JSON fixture → parse back →
//! validate. Set `COSM_BLESS=1` to rewrite the fixtures after a deliberate
//! schema change (and review the diff: the fixtures document the JSON shape).

mod common;

use crush_cosm::*;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.cosm.json"))
}

/// The built module, its pretty JSON and the checked-in fixture all agree.
fn check_fixture(name: &str, module: &CosmModule) {
    let json = module.to_json_pretty() + "\n";
    let path = fixture_path(name);
    if std::env::var_os("COSM_BLESS").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let on_disk =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        json, on_disk,
        "{name}: fixture drifted (COSM_BLESS=1 to rewrite)"
    );
    let parsed = CosmModule::from_json(&on_disk).unwrap();
    assert_eq!(&parsed, module, "{name}: JSON round-trip");
    assert_eq!(
        CosmModule::from_json(&module.to_json()).unwrap(),
        *module,
        "{name}: compact round-trip"
    );
}

#[test]
fn rust_point_round_trips_and_validates() {
    let m = common::rust_point();
    check_fixture("rust_point", &m);
    assert_eq!(validate(&m), vec![]);

    let p = m.kind("Point").unwrap();
    assert_eq!(p.field_slot("x"), Some(0));
    assert_eq!(p.field_slot("y"), Some(1));
    assert_eq!(
        p.effective_receiver(p.member("norm2").unwrap()),
        Some(Receiver::ExplicitRef)
    );
    assert_eq!(
        p.effective_receiver(p.member("shift").unwrap()),
        Some(Receiver::ExplicitRefMut)
    );
    assert_eq!(p.effective_receiver(p.member("new").unwrap()), None);
    assert_eq!(
        m.resolution_order("Point").unwrap(),
        vec![KindRef::from("Point")]
    );
}

#[test]
fn python_animals_round_trip_and_validate_against_prelude() {
    let m = common::python_animals();
    let prelude = common::python_prelude();
    check_fixture("python_animals", &m);
    check_fixture("python_prelude", &prelude);
    assert_eq!(validate_with(&m, &[&prelude]), vec![]);

    // On its own, `object` is unknown.
    let alone = validate(&m);
    assert_eq!(alone.len(), 1, "{alone:#?}");
    assert_eq!(alone[0].code, DiagnosticCode::UnknownParent);
    assert_eq!(alone[0].subject, "Animal");
    assert_eq!(alone[0].span, Some(Span::new(1, 1).with_file("example")));

    // MRO [Dog, Animal, object], as CPython: `Dog.__mro__`.
    let deps = [&prelude];
    let set = ModuleSet::new(&m, &deps);
    let mro = c3_mro(&KindRef::from("Dog"), &set).unwrap();
    assert_eq!(mro, ["Dog", "Animal", "object"].map(KindRef::from).to_vec());

    let dog = m.kind("Dog").unwrap();
    assert_eq!(dog.hooks.hash, Some(HookImpl::Disabled));
    assert_eq!(
        dog.effective_receiver(dog.member("speak").unwrap()),
        Some(Receiver::BoundSelf)
    );
}

#[test]
fn crush_point_round_trips_and_validates() {
    let m = common::crush_point();
    check_fixture("crush_point", &m);
    assert_eq!(validate(&m), vec![]);
    let p = m.kind("Point").unwrap();
    // Crush's defaults (design doc, "Crush's own KindDecl").
    assert_eq!(p.semantics, Semantics::Reference);
    assert_eq!(p.resolution, Resolution::None);
    assert_eq!(p.receiver, Receiver::ExplicitRef);
    assert_eq!(p.construct, Construct::Literal);
    assert_eq!(p.equality, Equality::Structural);
}

#[test]
fn json_spellings_of_the_catalogue() {
    use serde_json::json;
    let m = common::rust_point();
    let v: serde_json::Value = serde_json::from_str(&m.to_json()).unwrap();
    let p = &v["kinds"][0];
    assert_eq!(
        p["storage"],
        json!({"sealed": {"fields": [{"name": "x", "type": "Int"}, {"name": "y", "type": "Int"}]}})
    );
    assert_eq!(p["semantics"], json!({"value": "move"}));
    assert_eq!(p["construct"], json!("literal"));
    assert_eq!(
        p["members"][0],
        json!({"name": "new", "span": {"line": 5, "col": 5}, "kind": "static", "value": {"fn": "Point::new"}})
    );
    assert_eq!(p["members"][2]["receiver"], json!("explicit_ref_mut"));
    assert_eq!(
        p["hooks"],
        json!({"op": {"+": {"fn": "<Point as Add>::add"}}, "eq": "derived", "clone": "derived"})
    );

    let a: serde_json::Value = serde_json::from_str(&common::python_animals().to_json()).unwrap();
    assert_eq!(
        a["kinds"][0]["construct"],
        json!({"py_new_init": {"init_fn": "Animal.__init__"}})
    );
    assert_eq!(
        a["kinds"][0]["members"][0]["value"],
        json!({"const": "animal"})
    );
    assert_eq!(a["kinds"][1]["hooks"]["hash"], json!("disabled"));
}

#[test]
fn constants_keep_int_float_distinct() {
    let mut k = KindDecl::crush("C");
    k.members = vec![
        MemberDecl::static_const("i", Const::Int(3)),
        MemberDecl::static_const("f", Const::Float(3.0)),
        MemberDecl::static_const("n", Const::Null),
        MemberDecl::static_const("b", Const::Bool(true)),
        MemberDecl::static_const("s", Const::Str("3".into())),
    ];
    if let Storage::Sealed { fields } = &mut k.storage {
        let mut f = FieldDecl::new("z", CosmType::Float);
        f.default = Some(Const::Float(0.5));
        fields.push(f);
    }
    let m = CosmModule {
        kinds: vec![k],
        interfaces: vec![],
    };
    let back = CosmModule::from_json(&m.to_json()).unwrap();
    assert_eq!(back, m);
}

#[test]
fn bad_json_names_the_path() {
    let err = CosmModule::from_json(
        r#"{"kinds":[{"name":"P","home_lang":"crush","storage":"open_dict","semantics":"reference",
            "resolution":"mro","receiver":"explicit_ref","construct":"literal","equality":"identity"}]}"#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("kinds[0].resolution"), "{msg}");
    assert!(msg.contains("unknown variant `mro`"), "{msg}");

    let err = CosmModule::from_json(r#"{"kinds":[],"classes":[]}"#).unwrap_err();
    assert!(err.to_string().contains("unknown field `classes`"), "{err}");
}
