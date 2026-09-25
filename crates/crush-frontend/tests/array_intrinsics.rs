//! `array.push(a, x)` / `array.pop(a)` lower to the ARR_PUSH / ARR_POP
//! instructions, not to a `cap_call "array.push"`.
//!
//! The parser emits these dotted calls as `Expression::CapabilityCall`, but
//! the compiler only recognised them in the `Expression::Call` branch, so they
//! compiled to capability calls no backend implements ("unknown capability:
//! array.push"). nanovm's `sbl_core.crush` — ported as crush-lang-sdk's
//! `system.*` SBL (CRUSH-122) — depends on both.

use crush_frontend::compile_crush_source;

fn ops(src: &str) -> Vec<(String, String)> {
    let program = compile_crush_source(src).expect("compiles");
    let main = &program.functions["main"];
    main.body
        .iter()
        .map(|i| {
            let name = i.args.get("name").and_then(|n| n.as_str()).unwrap_or("");
            (i.op.clone(), name.to_string())
        })
        .collect()
}

#[test]
fn array_push_and_pop_lower_to_instructions() {
    let ops = ops("fn main() { let r = []; array.push(r, 1); print(array.pop(r)); }");
    let op_names: Vec<&str> = ops.iter().map(|(op, _)| op.as_str()).collect();
    assert!(op_names.contains(&"array_push"), "{ops:?}");
    assert!(op_names.contains(&"array_pop"), "{ops:?}");
    assert!(
        !ops.iter()
            .any(|(op, name)| op == "cap_call" && name.starts_with("array.")),
        "{ops:?}"
    );
}

#[test]
fn array_push_checks_its_arity() {
    assert!(compile_crush_source("fn main() { let r = []; array.push(r); }").is_err());
    assert!(compile_crush_source("fn main() { let r = []; array.pop(r, 1); }").is_err());
}

#[test]
fn a_variable_named_array_keeps_method_call_semantics() {
    // `array` is a declared variable here, so `array.push(x)` is a method
    // call on it (receiver + 1 arg), not the two-argument intrinsic.
    let ops = ops("fn main() { let array = []; array.push(1); }");
    assert!(
        ops.iter()
            .any(|(op, name)| op == "cap_call" && name == "push"),
        "{ops:?}"
    );
}
