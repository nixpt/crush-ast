//! Integration tests for crush-aot: full compile → load → call pipeline.

use crush_aot::{AotCompiler, Module};
use crush_vm::RuntimeValue;

// ── Full pipeline tests ───────────────────────────────────────────────────

#[test]
fn test_aot_int_return() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 42; }", "test_int_return")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Int(42));
}

#[test]
fn test_aot_float_return() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 3.14; }", "test_float_return")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Float(3.14));
}

#[test]
fn test_aot_bool_true() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return true; }", "test_bool_true")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_bool_false() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return false; }", "test_bool_false")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(false));
}

#[test]
fn test_aot_arithmetic_add() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { let x = 40; let y = 2; return x + y; }", "test_add")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Int(42));
}

#[test]
fn test_aot_arithmetic_sub() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 100 - 58; }", "test_sub")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Int(42));
}

#[test]
fn test_aot_arithmetic_mul() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 6 * 7; }", "test_mul")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Int(42));
}

#[test]
fn test_aot_arithmetic_div() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 100 / 4; }", "test_div")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Int(25));
}

#[test]
fn test_aot_comparison_eq() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 42 == 42; }", "test_eq")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_comparison_lt() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 10 < 20; }", "test_lt")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_comparison_gt() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return 100 > 50; }", "test_gt")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_logical_and() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return true && true; }", "test_and")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_logical_or() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return false || true; }", "test_or")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_logical_not() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return !false; }", "test_not")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Bool(true));
}

#[test]
fn test_aot_null() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return null; }", "test_null")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Null);
}

#[test]
fn test_aot_string_return() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source("fn main() { return \"hello\"; }", "test_string")
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::String("hello".to_string()));
}

#[test]
fn test_aot_multi_line() {
    let compiler = AotCompiler::new();
    let so_path = compiler
        .compile_source(
            "fn main() {
                let a = 10;
                let b = 20;
                let c = a + b;
                return c + 12;
            }",
            "test_multi",
        )
        .expect("compile_source failed");

    let module = Module::load(&so_path).expect("Module::load failed");
    let result = module.call_main().expect("call_main failed");

    assert_eq!(result, RuntimeValue::Int(42));
}

#[test]
fn test_aot_eval_i64() {
    let result = crush_aot::eval_i64("fn main() { return 99; }").expect("eval_i64 failed");
    assert_eq!(result, 99);
}

#[test]
fn test_aot_eval_bool() {
    let result = crush_aot::eval_bool("fn main() { return 5 > 3; }").expect("eval_bool failed");
    assert_eq!(result, true);
}

// ── Cross-tier comparison: AOT vs FastVM ────────────────────────────────

#[test]
fn test_aot_vs_fastvm_arithmetic() {
    let source = "fn main() { let x = 10; let y = 32; return x + y; }";

    // AOT
    let compiler = AotCompiler::new();
    let so_path = compiler.compile_source(source, "cmp_arith").unwrap();
    let module = Module::load(&so_path).unwrap();
    let aot_result = module.call_main().unwrap();

    // FastVM
    let casm = crush_frontend::compile_crush_source(source).unwrap();
    let fastvm_result = crush_vm::run_fastvm(&casm).unwrap();
    let fvm_val = match fastvm_result {
        crush_vm::fastvm::FastYield::Finished(Some(v)) => v,
        crush_vm::fastvm::FastYield::Value(v) => v,
        other => panic!("Unexpected FastVM result: {:?}", other),
    };

    assert_eq!(aot_result, fvm_val);
}

#[test]
fn test_aot_vs_fastvm_bool_logic() {
    let source = "fn main() { return true && !false; }";

    let compiler = AotCompiler::new();
    let so_path = compiler.compile_source(source, "cmp_bool").unwrap();
    let module = Module::load(&so_path).unwrap();
    let aot_result = module.call_main().unwrap();

    let casm = crush_frontend::compile_crush_source(source).unwrap();
    let fastvm = crush_vm::run_fastvm(&casm).unwrap();
    let fvm_val = match fastvm {
        crush_vm::fastvm::FastYield::Finished(Some(v)) => v,
        crush_vm::fastvm::FastYield::Value(v) => v,
        other => panic!("Unexpected FastVM result: {:?}", other),
    };

    assert_eq!(aot_result, fvm_val);
}

// ── Cache test ──────────────────────────────────────────────────────────

#[test]
fn test_aot_cache_hit() {
    let compiler = AotCompiler::new();
    let source = "fn main() { return 777; }";

    // First compilation
    let path1 = compiler.compile_source(source, "cache_test").unwrap();
    // Second compilation — should be a cache hit (same hash)
    let path2 = compiler.compile_source(source, "cache_test").unwrap();

    assert_eq!(path1, path2, "Cache should return same .so path");
}
