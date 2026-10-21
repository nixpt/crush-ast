//! Exported *definitions* must survive lowering (CRUSH-266).
//!
//! `export function/const/class` used to lower to a bare
//! `Statement::Export { value: Var(name) }` with the declaration itself
//! dropped, so the CAST compiled to code that loaded an uninitialised slot.
//! `export_multi_test.rs` only asserts exported *names* reach CAST; these
//! tests assert the bodies/initialisers do too, and that an exported
//! function is callable end-to-end (which also exercises CRUSH-266a: loading
//! a program-level function name in the `Export` statement).

// The swc path only: the Boa lowering stubs module items (CRUSH-250).
#![cfg(not(feature = "boa-backend"))]

use crush_cast::{Expression, Statement};
use crush_lang_js::js_to_cast;

fn run_js(source: &str) -> String {
    let cast = js_to_cast(source, "js").expect("walk");
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .expect("CAST->CASM");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("CASM->CVM1");
    let quotas = crush_vm::Quotas {
        max_steps: 10_000_000,
        ..Default::default()
    };
    crush_vm::run_with_caps(&vm, &quotas, None)
        .expect("run")
        .output
}

fn main_body(source: &str) -> Vec<Statement> {
    js_to_cast(source, "js")
        .expect("walk")
        .functions
        .get("main")
        .expect("main")
        .body
        .clone()
}

#[test]
fn exported_function_is_hoisted_with_its_body() {
    let program = js_to_cast("export function double(x) { return x * 2; }", "js").expect("walk");
    let f = program
        .functions
        .get("double")
        .expect("exported function must be in Program.functions");
    assert!(
        f.body.iter().any(|s| matches!(s, Statement::Return { .. })),
        "exported function body must be lowered, got {:?}",
        f.body
    );
}

#[test]
fn exported_function_is_callable_end_to_end() {
    let out = run_js("export function double(x) { return x * 2; }\nconsole.log(double(21));");
    assert_eq!(out.trim(), "42");
}

#[test]
fn exported_var_keeps_its_initializer() {
    let body = main_body("export const k = 21;");
    let var_pos = body
        .iter()
        .position(|s| {
            matches!(s, Statement::VarDecl { name, value, .. }
            if name == "k" && matches!(value, Expression::IntLiteral { value: 21, .. }))
        })
        .expect("exported var must emit its VarDecl");
    let export_pos = body
        .iter()
        .position(|s| matches!(s, Statement::Export { name, .. } if name == "k"))
        .expect("exported var must still emit its Export");
    assert!(var_pos < export_pos, "definition must precede the Export");
}

#[test]
fn exported_var_runs_end_to_end() {
    let out = run_js("export const k = 21;\nconsole.log(k);");
    assert_eq!(out.trim(), "21");
}

#[test]
fn exported_multi_declarator_defines_every_name() {
    let body = main_body("export let a = 1, b = 2;");
    for name in ["a", "b"] {
        assert!(
            body.iter()
                .any(|s| matches!(s, Statement::VarDecl { name: n, .. } if n == name)),
            "missing VarDecl for {name}"
        );
        assert!(
            body.iter()
                .any(|s| matches!(s, Statement::Export { name: n, .. } if n == name)),
            "missing Export for {name}"
        );
    }
}

#[test]
fn exported_class_keeps_its_definition() {
    let body = main_body("export class Box { get() { return 1; } }");
    let var_pos = body
        .iter()
        .position(|s| {
            matches!(s, Statement::VarDecl { name, value, .. }
            if name == "Box" && matches!(value, Expression::ObjectLiteral { .. }))
        })
        .expect("exported class must emit its class VarDecl");
    let export_pos = body
        .iter()
        .position(|s| matches!(s, Statement::Export { name, .. } if name == "Box"))
        .expect("exported class must still emit its Export");
    assert!(var_pos < export_pos, "definition must precede the Export");
}

#[test]
fn exported_arrow_const_is_callable_end_to_end() {
    let out = run_js("export const double = (x) => x * 2;\nconsole.log(double(21));");
    assert_eq!(out.trim(), "42");
}

#[test]
fn default_export_named_function_is_hoisted() {
    let program =
        js_to_cast("export default function double(x) { return x * 2; }", "js").expect("walk");
    assert!(
        program.functions.contains_key("double"),
        "named default function must be hoisted, got {:?}",
        program.functions.keys().collect::<Vec<_>>()
    );
}
