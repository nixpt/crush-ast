//! CRUSH-149: the deprecated `crush_cast::cson` path still resolves to the
//! same types as `crush_cast::caison`.

#![allow(deprecated)]

#[test]
fn deprecated_cson_module_is_the_same_type() {
    fn takes_new(_: &crush_cast::caison::CaisonValue) {}
    let old: crush_cast::cson::CaisonValue = crush_cast::cson::CaisonValue::Null;
    takes_new(&old);
}
