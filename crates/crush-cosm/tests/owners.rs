//! Owner ids (V3b, #522; `docs/design/visibility-and-packages.md` §2
//! "Schema change"): a Kind's `module` and `package`, and the owning package
//! of each function (`fn_packages`). They are must-understand and need COSM
//! 0.2.0; a module without them is written exactly as before.

use crush_cosm::{
    COSM_BASE_VERSION, COSM_OWNERS_VERSION, COSM_VERSION, CosmModule, FnRef, check_readable_by,
};
use serde_json::{Value, json};

const POINT_FIXTURE: &str = include_str!("fixtures/crush_point.cosm.json");

/// The fixture's canonical compact JSON as every writer before V3b wrote it,
/// byte for byte.
const POINT_GOLDEN: &str = r#"{"kinds":[{"name":"Point","home_lang":"crush","span":{"file":"example","line":1,"col":1},"storage":{"sealed":{"fields":[{"name":"x","type":"Int"},{"name":"y","type":"Int"}]}},"semantics":"reference","resolution":"none","receiver":"explicit_ref","construct":"literal","members":[{"name":"new","span":{"line":3,"col":5},"kind":"static","value":{"fn":"Point::new"}},{"name":"norm2","span":{"line":4,"col":5},"kind":"method","fn":"Point::norm2"}],"conforms":[{"interface":"Shape","check":"nominal","members":[{"name":"area","kind":"method","fn":"<Point as Shape>::area"}],"span":{"file":"example","line":7,"col":1}}],"equality":"structural"}],"interfaces":[{"name":"Shape","home_lang":"crush","span":{"file":"example","line":6,"col":1},"members":[{"name":"area","receiver":"explicit_ref","returns":"Any"},{"name":"describe","receiver":"explicit_ref","returns":"Any","default":"Shape::describe"}],"satisfaction":"nominal"}]}"#;

fn point() -> CosmModule {
    CosmModule::from_json(POINT_FIXTURE).unwrap()
}

/// The fixture with an owner on `Point` and one function outside the root
/// package.
fn owned() -> CosmModule {
    let mut m = point();
    m.kinds[0].module = Some("geo".into());
    m.kinds[0].package = Some("pkg:geom".into());
    m.fn_packages
        .insert(FnRef::new("Point::norm2"), "pkg:geom".into());
    m
}

fn json_of(m: &CosmModule) -> Value {
    serde_json::from_str(&m.to_json()).unwrap()
}

fn load(v: &Value) -> Result<CosmModule, String> {
    CosmModule::from_json(&v.to_string()).map_err(|e| e.to_string())
}

#[test]
fn a_module_without_owners_is_written_byte_for_byte_as_before() {
    let m = point();
    assert!(!m.has_owners());
    assert_eq!(m.required_version(), COSM_BASE_VERSION);
    assert_eq!(m.to_json(), POINT_GOLDEN);
}

#[test]
fn a_module_with_owners_is_stamped_and_round_trips() {
    let m = owned();
    assert!(m.has_owners());
    assert_eq!(m.required_version(), COSM_OWNERS_VERSION);
    assert_eq!(COSM_OWNERS_VERSION, "0.2.0");
    let v = json_of(&m);
    assert_eq!(v["cosm_version"], json!("0.2.0"));
    assert_eq!(v["kinds"][0]["module"], json!("geo"));
    assert_eq!(v["kinds"][0]["package"], json!("pkg:geom"));
    assert_eq!(v["fn_packages"], json!({"Point::norm2": "pkg:geom"}));
    let back = CosmModule::from_json(&m.to_json()).unwrap();
    assert_eq!(back, m);
    assert_eq!(back.function_package("Point::norm2"), Some("pkg:geom"));
    assert_eq!(back.function_package("Point::new"), None, "root package");
}

#[test]
fn each_owner_field_alone_raises_the_version() {
    let mut a = point();
    a.kinds[0].module = Some("geo".into());
    let mut b = point();
    b.kinds[0].package = Some("dir:lib".into());
    let mut c = point();
    c.fn_packages.insert(FnRef::new("f"), "dir:lib".into());
    for m in [a, b, c] {
        assert_eq!(m.required_version(), COSM_OWNERS_VERSION, "{}", m.to_json());
        assert!(m.to_json().contains("\"cosm_version\":\"0.2.0\""));
    }
}

#[test]
fn a_module_with_only_function_owners_is_not_empty() {
    let mut m = CosmModule::new();
    assert!(m.is_empty());
    m.fn_packages
        .insert(FnRef::new("geo::f"), "pkg:geom".into());
    assert!(!m.is_empty());
    assert_eq!(
        m.to_json(),
        r#"{"cosm_version":"0.2.0","kinds":[],"interfaces":[],"fn_packages":{"geo::f":"pkg:geom"}}"#
    );
}

#[test]
fn a_reader_of_the_previous_version_refuses_owners_by_name() {
    // What a COSM 0.1.0 runtime (V3a, #529) does with today's stamp: the
    // same gate, at its version. It names the version it can't read.
    let v = json_of(&owned());
    let stamp = v["cosm_version"].as_str().unwrap();
    let err = check_readable_by(stamp, COSM_BASE_VERSION).unwrap_err();
    assert!(err.contains("needs COSM 0.2.0"), "{err}");
    assert!(err.contains("reads COSM up to 0.1.0"), "{err}");
    // And this reader takes it.
    assert!(check_readable_by(stamp, COSM_VERSION).is_ok());
}

#[test]
fn a_newer_reader_than_owners_is_still_refused_by_this_one() {
    let mut v = json_of(&owned());
    v["cosm_version"] = json!("0.3.0");
    let err = load(&v).unwrap_err();
    assert!(err.contains("needs COSM 0.3.0"), "{err}");
}

#[test]
fn an_unknown_key_beside_the_owner_fields_is_refused() {
    let mut v = json_of(&owned());
    v["kinds"][0]["owner"] = json!("x");
    let err = load(&v).unwrap_err();
    assert!(err.contains("unknown field `owner`"), "{err}");

    let mut v = json_of(&owned());
    v["fn_owners"] = json!({});
    let err = load(&v).unwrap_err();
    assert!(err.contains("unknown field `fn_owners`"), "{err}");
}

#[test]
fn an_explicit_null_owner_is_a_type_error_not_absent() {
    for key in ["module", "package"] {
        let mut v = json_of(&owned());
        v["kinds"][0][key] = Value::Null;
        let err = load(&v).unwrap_err();
        assert!(err.contains("invalid type: null"), "{key}: {err}");
        assert!(err.contains(&format!("kinds[0].{key}")), "{key}: {err}");
    }
    let mut v = json_of(&owned());
    v["fn_packages"] = Value::Null;
    let err = load(&v).unwrap_err();
    assert!(err.contains("invalid type: null"), "{err}");
    assert!(err.contains("fn_packages"), "{err}");

    let mut v = json_of(&owned());
    v["fn_packages"]["Point::norm2"] = Value::Null;
    let err = load(&v).unwrap_err();
    assert!(err.contains("invalid type: null"), "{err}");
}

#[test]
fn the_root_package_and_empty_ids_are_refused_spelled_out() {
    let mut v = json_of(&owned());
    v["kinds"][0]["package"] = json!("root");
    let err = load(&v).unwrap_err();
    assert!(err.contains("absent owner"), "{err}");

    let mut v = json_of(&owned());
    v["fn_packages"]["Point::norm2"] = json!("root");
    let err = load(&v).unwrap_err();
    assert!(
        err.contains("fn_packages.Point::norm2") && err.contains("absent owner"),
        "{err}"
    );

    for key in ["module", "package"] {
        let mut v = json_of(&owned());
        v["kinds"][0][key] = json!("");
        let err = load(&v).unwrap_err();
        assert!(err.contains("empty"), "{key}: {err}");
    }
    let mut v = json_of(&owned());
    v["fn_packages"]["Point::norm2"] = json!("");
    assert!(load(&v).unwrap_err().contains("empty"));
}

#[test]
fn owner_fields_are_never_ignorable() {
    for key in ["module", "package", "fn_packages"] {
        assert!(!crush_cosm::IGNORABLE_MEMBER_KEYS.contains(&key));
    }
}

/// The owner-id rule is one check on every route: what the reader refuses,
/// the writer refuses to write and `validate` reports.
#[test]
fn the_writer_and_validate_refuse_what_the_reader_refuses() {
    for bad in ["root", "", "lib", "pkg:", "dir:", "pkg:a/b", "dir:x\\y"] {
        let mut m = point();
        m.kinds[0].package = Some(bad.into());
        let err = m.try_to_json().unwrap_err();
        assert!(err.contains("kinds[0].package"), "{bad:?}: {err}");
        let d = crush_cosm::validate(&m);
        assert!(
            d.iter()
                .any(|d| d.code == crush_cosm::DiagnosticCode::InvalidOwnerId),
            "{bad:?}: {d:?}"
        );
        let mut v = json_of(&owned());
        v["kinds"][0]["package"] = json!(bad);
        assert!(load(&v).is_err(), "{bad:?} must not load");

        let mut m = point();
        m.fn_packages.insert(FnRef::new("f"), bad.into());
        assert!(m.try_to_json().unwrap_err().contains("fn_packages.f"), "{bad:?}");
    }
    let mut m = point();
    m.kinds[0].module = Some("a/b".into());
    assert!(m.try_to_json().is_err());
    for good in ["pkg:facet", "dir:lib", "dir:lib#2"] {
        let mut m = point();
        m.kinds[0].package = Some(good.into());
        let back = CosmModule::from_json(&m.try_to_json().unwrap()).unwrap();
        assert_eq!(back, m);
        assert!(crush_cosm::validate(&m).is_empty(), "{good}");
    }
}

#[test]
#[should_panic(expected = "kinds[0].package")]
fn to_json_panics_on_an_invalid_owner_id() {
    let mut m = point();
    m.kinds[0].package = Some("root".into());
    let _ = m.to_json();
}
