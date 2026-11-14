//! The COSM evolution rule (V3a, #521; visibility design decisions 5 and 7):
//! a module may carry `cosm_version`; a reader takes the same major and a
//! minor no greater than its own, and refuses anything newer by naming both
//! versions. Members are must-understand: an unknown key is an error, not
//! silently dropped.

use crush_cosm::{COSM_BASE_VERSION, COSM_VERSION, CosmModule, check_readable};
use serde_json::{Value, json};

const POINT_FIXTURE: &str = include_str!("fixtures/crush_point.cosm.json");

fn fixture() -> Value {
    serde_json::from_str(POINT_FIXTURE).unwrap()
}

fn with_version(v: Value) -> String {
    let mut m = fixture();
    m.as_object_mut().unwrap().insert("cosm_version".into(), v);
    m.to_string()
}

#[test]
fn the_base_version_and_an_absent_version_both_load() {
    let plain = CosmModule::from_json(POINT_FIXTURE).expect("no cosm_version");
    let stamped =
        CosmModule::from_json(&with_version(json!(COSM_BASE_VERSION))).expect("cosm_version 0.1.0");
    assert_eq!(plain, stamped);
}

#[test]
fn an_older_patch_or_same_minor_loads() {
    CosmModule::from_json(&with_version(json!("0.1.7"))).expect("same minor, other patch");
    CosmModule::from_json(&with_version(json!("0.0.3"))).expect("older minor");
}

#[test]
fn a_newer_minor_is_refused_naming_both_versions() {
    let err = CosmModule::from_json(&with_version(json!("0.2.0")))
        .unwrap_err()
        .to_string();
    assert!(err.contains("needs COSM 0.2.0"), "{err}");
    assert!(err.contains(COSM_VERSION), "{err}");
}

#[test]
fn another_major_is_refused() {
    let err = CosmModule::from_json(&with_version(json!("1.0.0")))
        .unwrap_err()
        .to_string();
    assert!(err.contains("needs COSM 1.0.0"), "{err}");
}

#[test]
fn a_malformed_version_is_refused() {
    for bad in [json!("0.1"), json!("x.y.z"), json!(""), json!(1)] {
        assert!(
            CosmModule::from_json(&with_version(bad.clone())).is_err(),
            "{bad} must not load"
        );
    }
}

#[test]
fn check_readable_is_the_same_gate() {
    assert!(check_readable("0.1.0").is_ok());
    assert!(check_readable("0.1.99").is_ok());
    assert!(check_readable("0.2.0").is_err());
    assert!(check_readable("1.1.0").is_err());
}

#[test]
fn an_unknown_member_key_is_refused_not_dropped() {
    // A newer writer adds `visibility` to a member: this reader must refuse
    // the module instead of loading it with the field silently lost.
    let mut m = fixture();
    m["kinds"][0]["members"][1]
        .as_object_mut()
        .unwrap()
        .insert("visibility".into(), json!("priv"));
    let err = CosmModule::from_json(&m.to_string())
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown key `visibility`"), "{err}");
    assert!(err.contains("norm2"), "names the member: {err}");
}

#[test]
fn a_key_of_another_member_kind_is_refused() {
    // `value` belongs to `static`; on a `method` it is unknown.
    let mut m = fixture();
    m["kinds"][0]["members"][1]
        .as_object_mut()
        .unwrap()
        .insert("value".into(), json!({"fn": "x"}));
    let err = CosmModule::from_json(&m.to_string())
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown key `value`"), "{err}");
}

#[test]
fn an_unknown_member_kind_still_names_the_variant() {
    let mut m = fixture();
    m["kinds"][0]["members"][1]["kind"] = json!("property");
    let err = CosmModule::from_json(&m.to_string())
        .unwrap_err()
        .to_string();
    assert!(err.contains("property"), "{err}");
}

#[test]
fn an_unknown_top_level_key_is_still_refused() {
    let mut m = fixture();
    m.as_object_mut()
        .unwrap()
        .insert("exports".into(), json!([]));
    let err = CosmModule::from_json(&m.to_string())
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown field `exports`"), "{err}");
}

#[test]
fn serialization_is_unchanged_while_nothing_needs_a_newer_version() {
    let m = CosmModule::from_json(POINT_FIXTURE).unwrap();
    assert_eq!(m.required_version(), COSM_BASE_VERSION);
    let out = m.to_json();
    assert!(!out.contains("cosm_version"), "{out}");
    let a: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(a, fixture(), "round trip is value-identical");
    // And a stamped input is written back unstamped (the stamp is derived,
    // not stored).
    let stamped = CosmModule::from_json(&with_version(json!("0.1.0"))).unwrap();
    assert_eq!(stamped.to_json(), out);
}
