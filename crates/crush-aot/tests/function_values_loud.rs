//! CRUSH-251 step 2 runs function values on the scheduler and the portable VM
//! only. AOT Rust, AOT C and FastVM (and so the JIT, which compiles from the
//! FastVM lowering) must refuse them — exact or loud, never silently wrong.

const PROGRAM: &str = "fn apply(f, v) {\n  return f(v)\n}\nfn main() {\n  print(apply(|x| x * 2, 21))\n}\n";
const APPLY_SPREAD: &str = "fn relay(f, xs) { return __crush_apply__(f, xs) }\nfn main() { return relay(null, []) }\n";

fn casm() -> casm::Program {
    crush_frontend::compile_crush_source(PROGRAM).expect("frontend")
}

#[test]
fn aot_rust_rejects_function_values() {
    let e = crush_aot::codegen::gen_rust_source(&casm()).expect_err("must not compile");
    assert!(e.ops.iter().any(|o| o.contains("make_closure")), "{:?}", e.ops);
    assert!(e.ops.iter().any(|o| o.contains("call_value")), "{:?}", e.ops);
}

#[test]
fn aot_c_rejects_function_values() {
    let e = crush_aot::codegen_c::gen_c_source(&casm()).expect_err("must not compile");
    assert!(e.ops.iter().any(|o| o.contains("make_closure")), "{:?}", e.ops);
    assert!(e.ops.iter().any(|o| o.contains("call_value")), "{:?}", e.ops);
}

#[test]
fn fastvm_rejects_function_values() {
    let e = crush_vm::fastvm::lower_program(&casm()).expect_err("must not lower");
    assert!(e.to_string().contains("make_closure") || e.to_string().contains("call_value"), "{e}");
}

#[test]
fn aot_rust_rejects_call_value_apply() {
    let program = crush_frontend::compile_crush_source(APPLY_SPREAD).expect("frontend");
    let e = crush_aot::codegen::gen_rust_source(&program).expect_err("must refuse");
    assert!(e.ops.iter().any(|o| o.contains("call_value_apply")), "{:?}", e.ops);
}

#[test]
fn aot_c_rejects_call_value_apply() {
    let program = crush_frontend::compile_crush_source(APPLY_SPREAD).expect("frontend");
    let e = crush_aot::codegen_c::gen_c_source(&program).expect_err("must refuse");
    assert!(e.ops.iter().any(|o| o.contains("call_value_apply")), "{:?}", e.ops);
}

#[test]
fn fastvm_rejects_call_value_apply() {
    let program = crush_frontend::compile_crush_source(APPLY_SPREAD).expect("frontend");
    let e = crush_vm::fastvm::lower_program(&program).expect_err("FastVM must refuse");
    assert!(e.to_string().contains("call_value_apply"), "{e}");
    // JIT only consumes LoweredProgram; FastVM's refusal keeps this opcode
    // from entering the JIT, so it has no path to execute or drop it silently.
}
