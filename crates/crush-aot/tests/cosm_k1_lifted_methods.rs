//! COSM K1.3 (CRUSH-zorro-42): FastVM, the JIT and the AOT backends (Rust and
//! C) don't implement COSM objects yet, so a program with a kind table — any
//! Crush program that declares a `struct`, `impl` or `trait` — is refused at
//! load/compile time, naming the engine and CRUSH-zorro-43, instead of
//! running its objects as maps.
//!
//! (Under COSM K1.2 these engines ran the lifted methods through qualified
//! calls. The scheduler VM and the portable VM still do: see crush-lang-sdk's
//! `cosm_k1_impl_trait` and `cosm_kind_table` tests.)

use crush_aot::AotCompiler;

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

fn casm() -> casm::Program {
    crush_frontend::compile_crush_source(PROGRAM).expect("frontend")
}

/// The same program with its kind table taken away: the engines still
/// compile and run it, so the refusal is about the kind table alone.
fn without_kind_table() -> casm::Program {
    let mut p = casm();
    p.cosm = None;
    p
}

fn assert_refused(err: &str, engine: &str) {
    let want = format!("COSM objects aren't supported on {engine} yet (CRUSH-zorro-43)");
    assert!(err.contains(&want), "want {want:?} in: {err}");
    assert!(err.contains("Kinds Point and interfaces Shape"), "{err}");
}

fn cc_available() -> bool {
    std::process::Command::new("gcc").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

#[test]
fn aot_rust_refuses_a_kind_table() {
    let err = AotCompiler::new().compile_casm(&casm(), "cosm_k1_rust").expect_err("refused");
    assert_refused(&format!("{err:#}"), "the Rust AOT backend");
    // Generation itself refuses, before rustc is involved.
    let err = crush_aot::codegen::gen_rust_source(&casm()).expect_err("refused");
    assert_eq!(err.backend, "Rust");
    assert!(crush_aot::codegen::gen_rust_source(&without_kind_table()).is_ok());
}

#[test]
fn aot_c_refuses_a_kind_table() {
    let err = crush_aot::codegen_c::gen_c_source(&casm()).expect_err("refused");
    assert_eq!(err.backend, "C");
    assert_refused(&err.to_string(), "the C AOT backend");
    assert!(crush_aot::codegen_c::gen_c_source(&without_kind_table()).is_ok());
    if cc_available() {
        let err = AotCompiler::new().compile_c(&casm(), "cosm_k1_c", "gcc").expect_err("refused");
        assert_refused(&format!("{err:#}"), "the C AOT backend");
    }
}

#[test]
fn fastvm_refuses_a_kind_table() {
    let err = crush_vm::run_fastvm(&casm()).expect_err("refused");
    assert_refused(&format!("{err:?}"), "FastVM or the JIT");
    assert!(crush_vm::run_fastvm(&without_kind_table()).is_ok());
}

/// The JIT runs FastVM's lowered program; the lowering is where a kind table
/// is refused, so no JIT code is ever compiled for one.
#[test]
fn jit_refuses_a_kind_table() {
    let Err(err) = crush_vm::fastvm::lower_program(&casm()) else {
        panic!("a kind table must be refused");
    };
    assert_refused(&err.to_string(), "FastVM or the JIT");
    let lowered = crush_vm::fastvm::lower_program(&without_kind_table()).expect("lower");
    let engine = crush_jit::JitEngine::new().expect("jit");
    assert!(engine.run(&lowered).is_ok());
}
