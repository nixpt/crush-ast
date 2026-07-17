use casm::{Program, Function, Instruction};
use crush_ptx::compiler::compile_program;
use std::collections::HashMap;

#[test]
fn test_compile_basic_ptx() {
    let mut func = Function {
        params: vec!["ptr_in".to_string(), "ptr_out".to_string()],
        locals: vec![],
        type_hints: None,
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
        type_hints: None,
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
        type_hints: None,
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

#[test]
fn test_q6_ops() {
    let mut func = Function {
        params: vec![],
        locals: vec![],
        type_hints: None,
        body: vec![],
    };

    // 1. push_float 1.0 (a)
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 1.0}) });
    // 2. push_float 2.0 (b)
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 2.0}) });
    // 3. push_float 3.0 (c)
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 3.0}) });
    // 4. fma (a*b + c)
    func.body.push(Instruction { op: "fma".into(), lang: None, meta: None, args: serde_json::json!({}) });
    
    // 5. cvt to s32
    func.body.push(Instruction { op: "cvt".into(), lang: None, meta: None, args: serde_json::json!({"type": "s32"}) });
    
    // 6. ptx_lane_id
    func.body.push(Instruction { op: "ptx_lane_id".into(), lang: None, meta: None, args: serde_json::json!({}) });
    
    // 7. ptx_shfl_sync_bfly
    // push mask
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 31.0}) }); // actually needs to be u32 but we'll cast or just use what we have, let's use a custom u32 load
    
    let mut program = Program::default();
    program.functions.insert("test_kernel".to_string(), func);

    // we won't test shfl thoroughly if we don't have push_int, but fma and cvt should be in the output
    let ptx = compile_program(&program).expect("Failed to compile");
    assert!(ptx.contains("fma.rn.f64"));
    assert!(ptx.contains("cvt.rni.s32.f64")); // PTX requires a rounding modifier float->int
    assert!(ptx.contains("%laneid"));
}

// Regression: ptxas rejects `div.f64`/`div.f32` without a rounding modifier
// ("Rounding modifier required for instruction 'div'") — found by actually running
// ptxas against emitted PTX, not just checking it compiles as Rust. Integer div needs
// no modifier; float div does. Confirmed against real ptxas (CUDA 12.9, sm_80): both
// forms below assemble to a valid cubin.
#[test]
fn test_float_div_has_rounding_modifier() {
    let mut func = Function { params: vec![], locals: vec![], type_hints: None, body: vec![] };
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 10.0}) });
    func.body.push(Instruction { op: "push_float".into(), lang: None, meta: None, args: serde_json::json!({"value": 4.0}) });
    func.body.push(Instruction { op: "div".into(), lang: None, meta: None, args: serde_json::json!({}) });
    let mut program = Program::default();
    program.functions.insert("div_kernel".to_string(), func);

    let ptx = compile_program(&program).expect("Failed to compile");
    assert!(ptx.contains("div.rn.f64"), "float div must carry a PTX rounding modifier:\n{ptx}");
}

// ── Way-3 spike (ZORRO-CRUSH-PTX-1): a real Q6_K dequant→GEMV kernel ────────────
// Proves the crush→PTX emitter can express a load-bearing quant kernel (Tiers 0-4:
// SIMT ids, typed sub-word loads, 6-bit unpack, fma accumulate, warp-shuffle reduce)
// and that the emitted text is a *ptxas-valid* sm_120 kernel — not just a Rust-level
// string. Correctness vs the byte-exact `dequantize_q6_k` oracle is gated in zorro
// (Phase 2), where the emitted PTX is executed on the GPU.
#[test]
fn test_q6k_gemv_emits_ptxas_valid_kernel() {
    let prog = crush_ptx::q6k_gemv_program();
    let ptx = compile_program(&prog).expect("Q6_K GEMV must compile to PTX");

    // Shape: the load-bearing subset must be present.
    assert!(ptx.contains(".visible .entry gemv_q6k_crush"), "entry point:\n{ptx}");
    assert!(ptx.contains("%tid.x") && ptx.contains("%ctaid.x") && ptx.contains("%ntid.x"), "SIMT ids");
    assert!(ptx.contains("ld.global.u8"), "Q6_K quant bytes are u8 sub-word loads");
    // f16 super-block scale: loaded as raw b16 then widened (ptxas rejects ld.global.f16).
    assert!(ptx.contains("ld.global.b16") && ptx.contains("cvt.f32.f16"), "f16 scale idiom");
    assert!(ptx.contains("mul.lo.s64"), "integer mul needs .lo");
    assert!(ptx.contains("and.b64") && ptx.contains("shl.b64"), "bitwise ops are width-typed");
    assert!(ptx.contains("shr.s64"), "arithmetic shift for the 6-bit unpack / sign-extend");
    assert!(ptx.contains("cvt.rn.f32.s64"), "int->float needs a rounding modifier");
    assert!(ptx.contains("fma.rn.f32"), "dequant·activation accumulate");
    assert!(ptx.contains("shfl.sync.bfly.b32"), "warp reduction");
    assert!(ptx.contains("st.global.f32"), "y[row] store");
    assert!(ptx.contains("bra L_"), "the strided column loop back-edge");

    // Real gate: if a ptxas is reachable, the emitted text must assemble for sm_120.
    let ptxas = ["/opt/cuda/bin/ptxas", "/usr/local/cuda/bin/ptxas", "ptxas"]
        .into_iter()
        .find(|p| std::process::Command::new(p).arg("--version").output().map(|o| o.status.success()).unwrap_or(false));
    match ptxas {
        None => eprintln!("(skip) no ptxas on this box — shape asserts only"),
        Some(ptxas) => {
            let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let src = format!("{dir}/target-crush-q6k.ptx");
            let out = format!("{dir}/target-crush-q6k.cubin");
            std::fs::write(&src, &ptx).unwrap();
            let res = std::process::Command::new(ptxas)
                .args(["-arch=sm_120", &src, "-o", &out])
                .output()
                .expect("run ptxas");
            let _ = std::fs::remove_file(&src);
            let _ = std::fs::remove_file(&out);
            assert!(
                res.status.success(),
                "ptxas -arch=sm_120 rejected the crush-emitted Q6_K GEMV:\n{}",
                String::from_utf8_lossy(&res.stderr)
            );
            eprintln!("crush-ptx OK: Q6_K GEMV assembles for sm_120 (ptxas exit 0)");
        }
    }
}
