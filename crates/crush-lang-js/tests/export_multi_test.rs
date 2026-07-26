//! Multi-declarator / multi-specifier export lowering (CRUSH-CI-HONESTY-1).
//!
//! `export let a, b` and `export { a, b }` used to emit only the first name —
//! the loop returned on the first iteration (`clippy::never_loop`). These tests
//! assert every exported name reaches CAST.

use crush_cast::Statement;
use crush_lang_js::js_to_cast;

fn export_names(source: &str) -> Vec<String> {
    let program = js_to_cast(source, "js").expect("js_to_cast");
    let main = program.functions.get("main").expect("main");
    main.body
        .iter()
        .filter_map(|s| match s {
            Statement::Export { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn export_let_multi_declarator_exports_every_name() {
    let names = export_names("export let a = 1, b = 2;\n");
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn export_named_multi_specifier_exports_every_name() {
    let names = export_names("const a = 1;\nconst b = 2;\nexport { a, b };\n");
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}
