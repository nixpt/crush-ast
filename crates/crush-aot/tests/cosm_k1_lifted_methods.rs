//! COSM K1.2 (CRUSH-zorro-41): methods lifted from Crush `impl` / `trait`
//! blocks are ordinary functions with qualified names (`Point::norm2`,
//! `Point::Shape::area`, `Shape::describe`). Qualified calls of them must give
//! the same answer on every engine — the qualified names must survive each
//! backend's function naming (AOT mangling, FastVM/JIT lowering).

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

const WANT: RuntimeValue = RuntimeValue::Int(25 + 100 + 4);

fn casm() -> casm::Program {
    crush_frontend::compile_crush_source(PROGRAM).expect("frontend")
}

fn cc_available() -> bool {
    std::process::Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[test]
fn aot_rust_runs_lifted_methods() {
    let so = AotCompiler::new().compile_casm(&casm(), "cosm_k1_rust").expect("rust backend");
    assert_eq!(Module::load(&so).expect("load").call_main().expect("call_main"), WANT);
}

#[test]
fn aot_c_runs_lifted_methods() {
    if !cc_available() {
        eprintln!("gcc not available; skipping");
        return;
    }
    let so = AotCompiler::new().compile_c(&casm(), "cosm_k1_c", "gcc").expect("c backend");
    assert_eq!(Module::load(&so).expect("load").call_main().expect("call_main"), WANT);
}

#[test]
fn fastvm_runs_lifted_methods() {
    match crush_vm::run_fastvm(&casm()).expect("fastvm") {
        FastYield::Finished(Some(v)) | FastYield::Value(v) => assert_eq!(v, WANT),
        other => panic!("unexpected FastVM result: {other:?}"),
    }
}

#[test]
fn jit_runs_lifted_methods() {
    let lowered = crush_vm::fastvm::lower_program(&casm()).expect("lower");
    let engine = crush_jit::JitEngine::new().expect("jit");
    match engine.run(&lowered).expect("jit run") {
        FastYield::Finished(Some(v)) | FastYield::Value(v) => assert_eq!(v, WANT),
        other => panic!("unexpected JIT result: {other:?}"),
    }
}
