//! Binding identity stays available to closure/capture tooling after lowering.
use crush_cast::{Expression, Statement};
use crush_lang_rust::rust_to_cast;

#[test]
fn declarations_and_references_keep_source_identity() {
    let program = rust_to_cast("fn main() { let x = 1; { let x = 2; } let f = || x; }").unwrap();
    let function = &program.functions["main"];
    let Statement::VarDecl {
        name: outer,
        meta: outer_meta,
        ..
    } = &function.body[0]
    else {
        panic!("outer let");
    };
    let Statement::If { then_body, .. } = &function.body[1] else {
        panic!("block");
    };
    let Statement::VarDecl {
        name: inner,
        meta: inner_meta,
        ..
    } = &then_body[0]
    else {
        panic!("inner let");
    };
    assert_ne!(outer, inner);
    assert_eq!(outer_meta["rust_binding"]["source_name"], "x");
    assert_eq!(outer_meta["rust_binding"]["function"], "main");
    assert_eq!(inner_meta["rust_binding"]["source_name"], "x");
    assert_ne!(
        outer_meta["rust_binding"]["scope_id"],
        inner_meta["rust_binding"]["scope_id"]
    );
    let Statement::VarDecl {
        value: Expression::Lambda { body, meta, .. },
        ..
    } = &function.body[2]
    else {
        panic!("closure");
    };
    assert!(meta.contains_key("rust_scope_id"));
    let Statement::Return {
        value: Some(Expression::Var { name, meta }),
        ..
    } = &body[0]
    else {
        panic!("capture");
    };
    assert_eq!(name, outer);
    assert_eq!(meta["rust_binding"], outer_meta["rust_binding"]);
    assert_eq!(function.meta["rust_bindings"].as_array().unwrap().len(), 3);
}

#[test]
fn initializer_uses_the_previous_binding() {
    let program = rust_to_cast("fn main() { let x = 1; let x = x; }").unwrap();
    let body = &program.functions["main"].body;
    let Statement::VarDecl { name: old, .. } = &body[0] else {
        panic!("first let");
    };
    let Statement::VarDecl {
        name: new,
        value: Expression::Var { name: read, .. },
        ..
    } = &body[1]
    else {
        panic!("second let");
    };
    assert_ne!(old, new);
    assert_eq!(old, read);
}

#[test]
fn generated_storage_names_cannot_collide_with_source_identifiers() {
    let program = rust_to_cast("fn main() { let __crush_rust_binding_0 = 1; }").unwrap();
    let Statement::VarDecl { name, .. } = &program.functions["main"].body[0] else {
        panic!("let");
    };
    assert_ne!(name, "__crush_rust_binding_0");
    assert!(syn::parse_str::<syn::Ident>(name).is_ok());
}

#[test]
fn nested_functions_are_loud_until_item_scopes_are_supported() {
    let error = rust_to_cast("fn main() {\n    fn inner() {}\n    inner();\n}").unwrap_err();
    assert!(
        error.to_string().contains("nested function at 2:5"),
        "{error:#}"
    );
}
