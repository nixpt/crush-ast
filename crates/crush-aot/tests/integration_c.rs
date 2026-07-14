//! Integration tests for the C backend — compile → dlopen → call → compare.

use crush_aot::{AotCompiler, Module};
use crush_vm::RuntimeValue;

// ── C codegen tests ───────────────────────────────────────────────────────

#[test]
fn test_c_codegen_int() {
    let source = r#"fn main() { return 42; }"#;
    let c_src = crush_aot::codegen_c::gen_c_source(
        &crush_frontend::compile_crush_source(source).unwrap()
    );
    assert!(c_src.contains("fn_main"));
    assert!(c_src.contains("crush_run"));
    assert!(c_src.contains("TAG_INT"));
    assert!(c_src.contains("42LL"));
}

#[test]
fn test_c_codegen_bool() {
    let source = r#"fn main() { return true; }"#;
    let c_src = crush_aot::codegen_c::gen_c_source(
        &crush_frontend::compile_crush_source(source).unwrap()
    );
    assert!(c_src.contains("TAG_BOOL"));
    assert!(c_src.contains("true"));
}

#[test]
fn test_c_codegen_float() {
    let source = r#"fn main() { return 3.14; }"#;
    let c_src = crush_aot::codegen_c::gen_c_source(
        &crush_frontend::compile_crush_source(source).unwrap()
    );
    assert!(c_src.contains("3.14"));
}

#[test]
fn test_c_codegen_arithmetic() {
    let source = r#"fn main() { return 40 + 2; }"#;
    let c_src = crush_aot::codegen_c::gen_c_source(
        &crush_frontend::compile_crush_source(source).unwrap()
    );
    assert!(c_src.contains("_add"));
}

#[test]
fn test_c_codegen_null() {
    let source = r#"fn main() { return null; }"#;
    let c_src = crush_aot::codegen_c::gen_c_source(
        &crush_frontend::compile_crush_source(source).unwrap()
    );
    assert!(c_src.contains("TAG_NULL"));
}

#[test]
fn test_c_codegen_has_entry_point() {
    let source = r#"fn main() { return 42; }"#;
    let c_src = crush_aot::codegen_c::gen_c_source(
        &crush_frontend::compile_crush_source(source).unwrap()
    );
    assert!(c_src.contains("visibility"));
    assert!(c_src.contains("crush_run"));
}

// ── C compiler tests (gcc) ────────────────────────────────────��───────────

#[test]
fn test_c_gcc_int() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source("fn main() { return 42; }").unwrap();
    let so_path = compiler.compile_c(&program, "test_c_gcc_int", "gcc").expect("gcc compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Int(42));
}

#[test]
fn test_c_gcc_bool() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source("fn main() { return true; }").unwrap();
    let so_path = compiler.compile_c(&program, "test_c_gcc_bool", "gcc").expect("gcc compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Bool(true));
}

#[test]
fn test_c_gcc_arithmetic() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source(
        "fn main() { let x = 10; let y = 32; return x + y; }"
    ).unwrap();
    let so_path = compiler.compile_c(&program, "test_c_gcc_add", "gcc").expect("gcc compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Int(42));
}

#[test]
fn test_c_gcc_comparison() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source(
        "fn main() { return 42 == 42; }"
    ).unwrap();
    let so_path = compiler.compile_c(&program, "test_c_gcc_eq", "gcc").expect("gcc compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Bool(true));
}

#[test]
fn test_c_gcc_logic() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source(
        "fn main() { return true && !false; }"
    ).unwrap();
    let so_path = compiler.compile_c(&program, "test_c_gcc_logic", "gcc").expect("gcc compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Bool(true));
}

#[test]
fn test_c_gcc_null() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source(
        "fn main() { return null; }"
    ).unwrap();
    let so_path = compiler.compile_c(&program, "test_c_gcc_null", "gcc").expect("gcc compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Null);
}

// ── C compiler tests (clang) ──────────────────────────────────────────────

#[test]
fn test_c_clang_int() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source("fn main() { return 42; }").unwrap();
    let so_path = compiler.compile_c(&program, "test_c_clang_int", "clang").expect("clang compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Int(42));
}

#[test]
fn test_c_clang_arithmetic() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source(
        "fn main() { return 100 - 58; }"
    ).unwrap();
    let so_path = compiler.compile_c(&program, "test_c_clang_sub", "clang").expect("clang compile failed");
    let module = Module::load(&so_path).expect("load failed");
    assert_eq!(module.call_main().unwrap(), RuntimeValue::Int(42));
}

// ── Cross-backend consistency tests ────────────────────────────────────────

#[test]
fn test_cross_c_gcc_vs_rust() {
    let source = "fn main() { let a = 12; let b = 34; return a + b; }";
    let program = crush_frontend::compile_crush_source(source).unwrap();
    let compiler = AotCompiler::new();

    let so_c = compiler.compile_c(&program, "cross_c_gcc", "gcc").unwrap();
    let so_rust = compiler.compile_casm(&program, "cross_rust").unwrap();

    let mod_c = Module::load(&so_c).unwrap();
    let mod_rust = Module::load(&so_rust).unwrap();

    assert_eq!(mod_c.call_main().unwrap(), mod_rust.call_main().unwrap());
}

#[test]
fn test_cross_c_clang_vs_rust() {
    let source = "fn main() { return 7 * 6; }";
    let program = crush_frontend::compile_crush_source(source).unwrap();
    let compiler = AotCompiler::new();

    let so_c = compiler.compile_c(&program, "cross_c_clang", "clang").unwrap();
    let so_rust = compiler.compile_casm(&program, "cross_rust2").unwrap();

    let mod_c = Module::load(&so_c).unwrap();
    let mod_rust = Module::load(&so_rust).unwrap();

    assert_eq!(mod_c.call_main().unwrap(), mod_rust.call_main().unwrap());
}

#[test]
fn test_cross_c_gcc_vs_clang() {
    let source = "fn main() { return 100 / 4; }";
    let program = crush_frontend::compile_crush_source(source).unwrap();
    let compiler = AotCompiler::new();

    let so_gcc = compiler.compile_c(&program, "cross_gcc", "gcc").unwrap();
    let so_clang = compiler.compile_c(&program, "cross_clang", "clang").unwrap();

    let mod_gcc = Module::load(&so_gcc).unwrap();
    let mod_clang = Module::load(&so_clang).unwrap();

    assert_eq!(mod_gcc.call_main().unwrap(), mod_clang.call_main().unwrap());
}

#[test]
fn test_cross_all_three_vs_fastvm() {
    let source = "fn main() { let x = 20; let y = 22; return x + y; }";
    let program = crush_frontend::compile_crush_source(source).unwrap();
    let compiler = AotCompiler::new();

    let so_rust = compiler.compile_casm(&program, "all_rust").unwrap();
    let so_gcc = compiler.compile_c(&program, "all_gcc", "gcc").unwrap();
    let so_clang = compiler.compile_c(&program, "all_clang", "clang").unwrap();

    let expected = Module::load(&so_rust).unwrap().call_main().unwrap();
    assert_eq!(Module::load(&so_gcc).unwrap().call_main().unwrap(), expected);
    assert_eq!(Module::load(&so_clang).unwrap().call_main().unwrap(), expected);

    let fv = crush_vm::run_fastvm(&program)
        .map_err(|e| format!("FastVM: {:?}", e))
        .unwrap();
    let fv_val = match fv {
        crush_vm::fastvm::FastYield::Finished(Some(v)) => v,
        crush_vm::fastvm::FastYield::Value(v) => v,
        _ => RuntimeValue::Null,
    };
    assert_eq!(fv_val, expected, "FastVM diverges from AOT");
}

// ── Cache test for C backend ──────────────────────────────────────────────

#[test]
fn test_c_cache_hit() {
    let compiler = AotCompiler::new();
    let program = crush_frontend::compile_crush_source("fn main() { return 9; }").unwrap();

    let path1 = compiler.compile_c(&program, "c_cache", "gcc").unwrap();
    let path2 = compiler.compile_c(&program, "c_cache", "gcc").unwrap();

    assert_eq!(path1, path2, "C cache should return same .so path");
}
