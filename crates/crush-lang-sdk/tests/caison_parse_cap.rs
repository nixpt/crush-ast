//! CRUSH-149: `caison.parse` is the capability; `cson.parse` is a deprecated
//! alias with the same handler (removed in 0.4).

use crush_lang_sdk::HostCapsBuilder;
use crush_vm::vm::Value;

#[test]
fn default_registry_has_both_names_and_they_agree() {
    let caps = HostCapsBuilder::new().build();
    let src = Value::Str("port: 8080\n".to_string());
    let mut out = Vec::new();
    for name in ["caison.parse", "cson.parse"] {
        let cap = caps.get(name).unwrap_or_else(|| panic!("{name} not registered"));
        let v = cap.call(vec![src.clone()]).unwrap().unwrap();
        match v {
            Value::Map(m) => out.push(format!("{:?}", m.borrow().get("port"))),
            other => panic!("{name}: expected map, got {other:?}"),
        }
    }
    assert_eq!(out[0], out[1]);
    assert!(out[0].contains("8080"), "{}", out[0]);
}
