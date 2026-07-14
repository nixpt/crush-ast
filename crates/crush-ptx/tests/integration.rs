use casm::{Program, Function, Instruction};
use crush_ptx::compiler::compile_program;
use std::collections::HashMap;

#[test]
fn test_compile_basic_ptx() {
    let mut func = Function {
        params: vec!["ptr_in".to_string(), "ptr_out".to_string()],
        locals: vec![],
        body: vec![],
    };

    // Construct a simple CASM program manually to test the PTX codegen
    // 1. load ptr_out
    func.body.push(Instruction { op: "load".into(), lang: None, meta: None, args: serde_json::json!({"name": "ptr_out"}) });
    // 2. load ptr_in
    func.body.push(Instruction { op: "load".into(), lang: None, meta: None, args: serde_json::json!({"name": "ptr_in"}) });
    // 3. ptx_ld_global (pops ptr_in)
    func.body.push(Instruction { op: "ptx_ld_global".into(), lang: None, meta: None, args: serde_json::json!({"type": "f64"}) });
    // 4. push_float 2.0
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 2.0}) });
    // 5. add (pops 2.0 and ptx_ld_global result)
    func.body.push(Instruction { op: "add".into(), lang: None, meta: None, args: serde_json::json!({}) });
    // 6. ptx_st_global (pops add result, pops ptr_out)
    func.body.push(Instruction { op: "ptx_st_global".into(), lang: None, meta: None, args: serde_json::json!({}) });
    // 7. ret
    func.body.push(Instruction { op: "ret".into(), lang: None, meta: None, args: serde_json::json!({}) });

    let mut program = Program::default();
    program.functions.insert("test_kernel".to_string(), func);

    let ptx = compile_program(&program).expect("Failed to compile valid PTX");
    println!("Emitted PTX:\n{}", ptx);

    assert!(ptx.contains(".visible .entry test_kernel"));
    assert!(ptx.contains(".param .u64 ptr_in"));
    assert!(ptx.contains(".param .u64 ptr_out"));
    assert!(ptx.contains("ld.param.u64"));
    assert!(ptx.contains("ld.global.f64"));
    assert!(ptx.contains("add.f64"));
    assert!(ptx.contains("st.global.f64"));
    assert!(ptx.contains("ret;"));
}

#[test]
fn test_unimplemented_opcode_hard_errors() {
    let mut func = Function {
        params: vec![],
        locals: vec![],
        body: vec![],
    };

    func.body.push(Instruction { op: "unknown_future_op".into(), lang: None, meta: None, args: serde_json::json!({}) });
    let mut program = Program::default();
    program.functions.insert("test_kernel".to_string(), func);

    let res = compile_program(&program);
    assert!(res.is_err(), "Expected an error for unimplemented opcode, got Ok");
    let err = res.unwrap_err();
    assert!(err.contains("HARD ERROR: Unimplemented opcode in crush-ptx backend: unknown_future_op"));
}

#[test]
fn test_loop_and_tid() {
    let mut func = Function {
        params: vec![],
        locals: vec![],
        body: vec![],
    };

    // Just testing that tid/ctaid and jmp work
    func.body.push(Instruction { op: "ptx_thread_idx_x".into(), lang: None, meta: None, args: serde_json::json!({}) });
    func.body.push(Instruction { op: "ptx_block_idx_x".into(), lang: None, meta: None, args: serde_json::json!({}) });
    func.body.push(Instruction { op: "jmp".into(), lang: None, meta: None, args: serde_json::json!({"target": 0}) });

    let mut program = Program::default();
    program.functions.insert("test_kernel".to_string(), func);

    let ptx = compile_program(&program).expect("Failed to compile");
    assert!(ptx.contains("mov.u32"));
    assert!(ptx.contains("%tid.x"));
    assert!(ptx.contains("%ctaid.x"));
    assert!(ptx.contains("bra L_0;"));
}

#[test]
fn test_crush_source_to_ptx() {
    let source = r#"
        fn add_kernel(a, b) {
            let x = a + b;
            return x;
        }
    "#;

    let program = crush_frontend::compile_crush_source(source).expect("Failed to compile source");
    println!("Compiled program: {:?}", program);
    let ptx = compile_program(&program).expect("Failed to emit PTX");
    
    assert!(ptx.contains(".visible .entry add_kernel"));
    assert!(ptx.contains("add.s64")); // Since crush default types map to s64 without specific annotations or traces
}
