//! Browser WebAssembly runtime for Crush.
//!
//! Compiles Crush source via `crush-lang-sdk::compile_crush_source` (parse +
//! polyglot-marshaling-prep + CASM lowering + casm-to-VM-program assembly —
//! the same pipeline `crush-run`/`crushc` use natively) and executes it via
//! `crush-vm`'s portable (green-thread) interpreter (`crush_vm::run`), not a
//! separate reimplementation.
//!
//! `@lang{}` polyglot blocks are unsupported here: `EXEC_LANG` needs to spawn
//! a subprocess, which doesn't exist in a browser sandbox. The VM returns a
//! capability-gated `VmError` for those, same as running with no `--polyglot`
//! grant natively — not a silent no-op.

use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct ExecutionResult {
    output: String,
    steps: usize,
    halted: bool,
}

/// Compile and run Crush source, returning `{ output, steps, halted }` as a
/// plain JS object, or throwing a string error (compile failure or VmError).
#[wasm_bindgen]
pub fn execute(source: &str) -> Result<JsValue, JsValue> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .map_err(|e| JsValue::from_str(&format!("compile error: {e}")))?;

    let quotas = crush_vm::Quotas::default();
    let result = crush_vm::run(&program, &quotas)
        .map_err(|e| JsValue::from_str(&format!("runtime error: {e}")))?;

    let out = ExecutionResult {
        output: result.output,
        steps: result.steps,
        halted: result.halted,
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| JsValue::from_str(&e.to_string()))
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
