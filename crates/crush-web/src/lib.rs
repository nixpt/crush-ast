//! Browser WebAssembly runtime for Crush.
//!
//! Two entry points into `crush-vm`'s portable (green-thread) interpreter
//! (`crush_vm::run`), not a separate reimplementation:
//!
//! - [`execute`]: compiles Crush source via
//!   `crush-lang-sdk::compile_crush_source` (parse + polyglot-marshaling-prep
//!   + CASM lowering + casm-to-VM-program assembly — the same pipeline
//!   `crush-run`/`crushc` use natively) and runs it, in one call.
//! - [`run_blob`]: skips compilation entirely and runs a pre-compiled `.cvm`
//!   blob (`crush_vm::Program::to_blob()`'s format — the same file
//!   `crush-pkg build` writes to `target/<name>.cvm`). Ship that instead of
//!   raw source to avoid recompiling in the browser on every load.
//!
//! `@lang{}` polyglot blocks are unsupported either way: `EXEC_LANG` needs to
//! spawn a subprocess, which doesn't exist in a browser sandbox. The VM
//! returns a capability-gated `VmError` for those, same as running with no
//! `--polyglot` grant natively — not a silent no-op.

use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct ExecutionResult {
    output: String,
    steps: usize,
    halted: bool,
}

fn run_program(program: &crush_vm::Program) -> Result<JsValue, JsValue> {
    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run(program, &quotas)
        .map_err(|e| JsValue::from_str(&format!("runtime error: {e}")))?;

    let out = ExecutionResult {
        output: result.output,
        steps: result.steps,
        halted: result.halted,
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compile and run Crush source, returning `{ output, steps, halted }` as a
/// plain JS object, or throwing a string error (compile failure or VmError).
#[wasm_bindgen]
pub fn execute(source: &str) -> Result<JsValue, JsValue> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .map_err(|e| JsValue::from_str(&format!("compile error: {e}")))?;
    run_program(&program)
}

/// Run a pre-compiled `.cvm` blob (`crush-pkg build`'s `target/<name>.cvm`,
/// or any `crush_vm::Program::to_blob()` output). No parsing or compilation
/// happens here — this is strictly cheaper than [`execute`] when the source
/// was already compiled ahead of time. Returns `{ output, steps, halted }`,
/// or throws a string error (bad blob format or VmError).
#[wasm_bindgen]
pub fn run_blob(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let program = crush_vm::Program::from_blob(bytes)
        .map_err(|e| JsValue::from_str(&format!("blob error: {e}")))?;
    run_program(&program)
}

/// Compile Crush source without running it. Returns nothing on success;
/// throws the compiler's error message on failure. Cheaper than `execute`
/// for a "check as you type" editor use case.
#[wasm_bindgen]
pub fn check(source: &str) -> Result<(), JsValue> {
    crush_lang_sdk::compile::compile_crush_source(source)
        .map(|_| ())
        .map_err(|e| JsValue::from_str(&format!("compile error: {e}")))
}

/// Route Rust panics to `console.error` instead of an opaque wasm trap.
/// Call once at startup.
#[wasm_bindgen]
pub fn init_panic_hook() {
    #[cfg(feature = "panic-hook")]
    console_error_panic_hook::set_once();
}
