//! COSM K1.3 (CRUSH-zorro-42): every engine applies one rule to a program's
//! kind table (`casm::cosm_unsupported_reason`). A Kind that behaves exactly as
//! a plain map — Crush's defaults (`reference`, `structural`, `literal`; no
//! hooks, accessors or field defaults) — runs, and every engine gives the same
//! answer as the scheduler VM; methods lifted from `impl` / `trait` blocks are
//! ordinary functions with qualified names (`Point::norm2`,
//! `Point::Shape::area`, `Shape::describe`), which must survive each backend's
//! function naming (AOT mangling, FastVM/JIT lowering). A Kind with any other
//! strategy is refused at load/compile time, naming the engine, the Kind, the
//! strategy and CRUSH-zorro-43, instead of running as a map.

use crush_aot::{AotCompiler, Module};
use crush_vm::RuntimeValue;
use crush_vm::fastvm::FastYield;

const PROGRAM: &str = r#"
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
}
trait Shape { fn area(self); fn sides(self) { return 4; } }
impl Shape for Point { fn area(self) { return 100; } }
fn main() {
    let p = Point::new(3, 4)
    return Point::norm2(p) + Point::Shape::area(p) + Shape::sides(p)
}
"#;

const WANT: i64 = 25 + 100 + 4;

/// The program as compiled: its kind table has only Crush's defaults.
fn casm() -> casm::Program {
    let p = crush_frontend::compile_crush_source(PROGRAM).expect("frontend");
    assert!(p.kind_table().is_some(), "the program carries a kind table");
    p
}

/// The same program with `Point` declared `value(move)` — a strategy a map
/// can't honour. (The frontend refuses `@kind(semantics: value(move))` at
/// compile time, so the table is edited the way another producer would emit it.)
fn casm_value_move() -> casm::Program {
    let mut p = casm();
    let mut v = serde_json::to_value(p.cosm.as_ref().unwrap()).unwrap();
    v["kinds"][0]["semantics"] = serde_json::json!({"value": "move"});
    p.cosm = Some(serde_json::from_value(v).unwrap());
    p
}

/// The reference answer: the scheduler VM.
fn scheduler_answer() -> i64 {
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm()).expect("casm_to_vm");
    match crush_vm::run(&vm, &crush_vm::Quotas::default()).expect("scheduler").return_value {
        Some(crush_vm::vm::Value::Int(n)) => n,
        other => panic!("unexpected scheduler result: {other:?}"),
    }
}

fn cc_available() -> bool {
    std::process::Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn assert_refused(err: &str, engine: &str) {
    let want = format!("COSM objects aren't supported on {engine} yet (CRUSH-zorro-43)");
    assert!(err.contains(&want), "want {want:?} in: {err}");
    assert!(err.contains("`Point` declares semantics: value(move)"), "{err}");
}

#[test]
fn the_scheduler_vm_runs_the_lifted_methods() {
    assert_eq!(scheduler_answer(), WANT);
}

#[test]
fn aot_rust_runs_lifted_methods_like_the_scheduler_vm() {
    let so = AotCompiler::new().compile_casm(&casm(), "cosm_k1_rust").expect("rust backend");
    let got = Module::load(&so).expect("load").call_main().expect("call_main");
    assert_eq!(got, RuntimeValue::Int(scheduler_answer()));
}

#[test]
fn aot_c_runs_lifted_methods_like_the_scheduler_vm() {
    if !cc_available() {
        eprintln!("gcc not available; skipping");
        return;
    }
    let so = AotCompiler::new().compile_c(&casm(), "cosm_k1_c", "gcc").expect("c backend");
    let got = Module::load(&so).expect("load").call_main().expect("call_main");
    assert_eq!(got, RuntimeValue::Int(scheduler_answer()));
}

#[test]
fn fastvm_runs_lifted_methods_like_the_scheduler_vm() {
    match crush_vm::run_fastvm(&casm()).expect("fastvm") {
        FastYield::Finished(Some(v)) | FastYield::Value(v) => {
            assert_eq!(v, RuntimeValue::Int(scheduler_answer()))
        }
        other => panic!("unexpected FastVM result: {other:?}"),
    }
}

#[test]
fn jit_runs_lifted_methods_like_the_scheduler_vm() {
    let lowered = crush_vm::fastvm::lower_program(&casm()).expect("lower");
    let engine = crush_jit::JitEngine::new().expect("jit");
    match engine.run(&lowered).expect("jit run") {
        FastYield::Finished(Some(v)) | FastYield::Value(v) => {
            assert_eq!(v, RuntimeValue::Int(scheduler_answer()))
        }
        other => panic!("unexpected JIT result: {other:?}"),
    }
}

#[test]
fn the_scheduler_vm_refuses_a_kind_maps_cant_represent() {
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm_value_move()).expect("casm_to_vm");
    let err = crush_vm::run(&vm, &crush_vm::Quotas::default()).expect_err("refused");
    assert_refused(&err.to_string(), "this VM");
}

#[test]
fn aot_rust_refuses_a_kind_maps_cant_represent() {
    let err = AotCompiler::new().compile_casm(&casm_value_move(), "cosm_k1_rust_vm").expect_err("refused");
    assert_refused(&format!("{err:#}"), "the Rust AOT backend");
    // Generation itself refuses, before rustc is involved.
    let err = crush_aot::codegen::gen_rust_source(&casm_value_move()).expect_err("refused");
    assert_eq!(err.backend, "Rust");
}

#[test]
fn aot_c_refuses_a_kind_maps_cant_represent() {
    let err = crush_aot::codegen_c::gen_c_source(&casm_value_move()).expect_err("refused");
    assert_eq!(err.backend, "C");
    assert_refused(&err.to_string(), "the C AOT backend");
    if cc_available() {
        let err = AotCompiler::new().compile_c(&casm_value_move(), "cosm_k1_c_vm", "gcc").expect_err("refused");
        assert_refused(&format!("{err:#}"), "the C AOT backend");
    }
}

#[test]
fn fastvm_refuses_a_kind_maps_cant_represent() {
    let err = crush_vm::run_fastvm(&casm_value_move()).expect_err("refused");
    assert_refused(&format!("{err:?}"), "FastVM or the JIT");
}

/// The JIT runs FastVM's lowered program; the lowering is where such a kind
/// table is refused, so no JIT code is ever compiled for it.
#[test]
fn jit_refuses_a_kind_maps_cant_represent() {
    let Err(err) = crush_vm::fastvm::lower_program(&casm_value_move()) else {
        panic!("a value(move) Kind must be refused");
    };
    assert_refused(&err.to_string(), "FastVM or the JIT");
}
