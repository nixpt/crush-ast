//! # crush-aot
//!
//! Ahead-of-Time compiler for the Crush language.
//!
//! Compiles Crush source to native shared libraries (`.so`/`.dylib`/`.dll`)
//! by transpiling CASM to Rust, then invoking `rustc`.
//!
//! ## Quick start
//!
//! ```ignore
//! use crush_aot::AotCompiler;
//!
//! let compiler = AotCompiler::new();
//! let so_path = compiler.compile_source("fn main() { return 42; }", "answer")?;
//!
//! let module = crush_aot::Module::load(&so_path)?;
//! let result = module.call_main()?;
//! assert_eq!(result.as_int(), Some(42));
//! ```
//!
//! ## How it works
//!
//! 1. `crush_frontend::compile_crush_source()` → `casm::Program`
//! 2. `codegen::gen_rust_source()` → self-contained Rust source with `loop { match pc { ... } }` dispatch
//! 3. `rustc --crate-type cdylib` → native `.so`
//! 4. `Module::load()` → `libloading` wrapper, calls `crush_run()` C ABI entry point

pub mod codegen;
pub mod codegen_c;
pub mod compiler;
pub mod loader;

pub use compiler::AotCompiler;
pub use loader::Module;

/// Compile Crush source to native code, load it, and extract an i64 result.
///
/// Convenience function for the common case of calling `main()`.
pub fn eval_i64(source: &str) -> anyhow::Result<i64> {
    let compiler = AotCompiler::new();
    let so_path = compiler.compile_source(source, "eval")?;
    let module = Module::load(so_path)?;
    let result = module.call_main()?;
    result.as_int().ok_or_else(|| anyhow::anyhow!("Expected i64, got {:?}", result))
}

/// Compile Crush source to native code, load it, and extract an f64 result.
pub fn eval_f64(source: &str) -> anyhow::Result<f64> {
    let compiler = AotCompiler::new();
    let so_path = compiler.compile_source(source, "eval")?;
    let module = Module::load(so_path)?;
    let result = module.call_main()?;
    match result {
        crush_vm::RuntimeValue::Float(f) => Ok(f),
        crush_vm::RuntimeValue::Int(i) => Ok(i as f64),
        other => anyhow::bail!("Expected number, got {:?}", other),
    }
}

/// Compile Crush source to native code, load it, and extract a bool result.
pub fn eval_bool(source: &str) -> anyhow::Result<bool> {
    let compiler = AotCompiler::new();
    let so_path = compiler.compile_source(source, "eval")?;
    let module = Module::load(so_path)?;
    let result = module.call_main()?;
    result.as_bool().ok_or_else(|| anyhow::anyhow!("Expected bool, got {:?}", result))
}
