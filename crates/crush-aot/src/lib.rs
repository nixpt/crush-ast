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
mod method_support;
pub mod names;

pub use compiler::AotCompiler;

/// CASM a backend can't translate. Each entry names the op (or
/// `cap_call '<name>'`), the function and the instruction index. Generation
/// fails on these instead of emitting a no-op or a null stub, which compiled
/// fine and then silently did nothing — e.g. a dropped `@python` block
/// (CRUSH-132, #72).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the {backend} AOT backend cannot compile: {}", .ops.join("; "))]
pub struct UnsupportedOps {
    pub backend: &'static str,
    pub ops: Vec<String>,
}
pub use loader::Module;

/// COSM K1.3 (CRUSH-zorro-42): AOT doesn't compile Kinds yet. A program whose
/// kind table declares a Kind that can't run as a plain map is refused before
/// any code is generated (the rule every engine shares,
/// `casm::cosm_unsupported_reason`); one whose Kinds all can compiles as before.
fn refuse_cosm(program: &casm::Program, backend: &'static str) -> Result<(), UnsupportedOps> {
    program
        .refuse_cosm(&format!("the {backend} AOT backend"))
        .map_err(|msg| UnsupportedOps {
            backend,
            ops: vec![msg],
        })
}

/// issue #161: AOT doesn't run green tasks. A program that `spawn`s is refused
/// before any code is generated, naming `spawn` — not the `make_closure` a
/// `spawn` compiles through, and never silently skipping the task. `yield` and
/// `await` without `spawn` are no-ops (lowered to nothing; `await v` on a
/// non-handle returns `v`, issue #251), not refusals.
fn refuse_green_task(program: &casm::Program, backend: &'static str) -> Result<(), UnsupportedOps> {
    program
        .refuse_green_task(&format!("the {backend} AOT backend"))
        .map_err(|msg| UnsupportedOps {
            backend,
            ops: vec![msg],
        })
}

/// CRUSH-234: the ten `ai_*` opcodes name host capabilities (`ai_native.*`)
/// that a standalone native binary has no way to be granted or to serve, so
/// the old behaviour — push a made-up `{ok, kind, echo}` stub and carry on —
/// silently invented model output. AOT now refuses them at compile time, naming
/// the op. Both spellings the frontend/CASM use are covered (`ai_tool_chain` /
/// `ai_toolchain`, `ai_goal_decl` / `ai_goal_declaration`).
const AI_OPS: &[&str] = &[
    "ai_query",
    "ai_synthesize",
    "ai_agent_delegation",
    "ai_semantic_match",
    "ai_learning_loop",
    "ai_context_aware",
    "ai_toolchain",
    "ai_tool_chain",
    "ai_goal_declaration",
    "ai_goal_decl",
    "ai_progress_update",
    "ai_knowledge_sharing",
];

/// #423: the DOM opcodes are the same kind of op — the VMs dispatch them to a
/// granted `dom_native.*` host handler (and refuse them without one, CRUSH-378)
/// — but AOT kept the invented `{ok: true, kind, echo}` stub, so a program the
/// VM refuses "succeeded" natively with no grant check at all. Refused at
/// compile time like the AI ops. Covers the CASM names the frontend emits
/// (`dom_query`, `dom_mutate`, `dom_event_listener`) and the per-kind names
/// the codegen had stub arms for.
const DOM_OPS: &[&str] = &[
    "dom_query",
    "dom_mutate",
    "dom_event_listener",
    "dom_get",
    "dom_set",
    "dom_create",
    "dom_remove",
    "dom_child",
    "dom_parent",
    "dom_attr",
    "dom_text",
    "dom_event",
];

/// Refuse the host-capability op families (AI, DOM) that an AOT binary can't
/// be granted, naming each occurrence.
fn refuse_host_ops(program: &casm::Program, backend: &'static str) -> Result<(), UnsupportedOps> {
    let mut ops = Vec::new();
    for (name, func) in &program.functions {
        for (i, instr) in func.body.iter().enumerate() {
            let op = instr.op.as_str();
            if AI_OPS.contains(&op) {
                ops.push(format!(
                    "`{op}` (fn {name}, instruction {i}): AI ops need a granted `ai_native.*` host capability, which AOT binaries cannot be given; run it on the VM with the AI grant instead"
                ));
            } else if DOM_OPS.contains(&op) {
                ops.push(format!(
                    "`{op}` (fn {name}, instruction {i}): DOM ops need a granted `dom_native.*` host capability, which AOT binaries cannot be given; run it on the VM with a DOM host registered instead"
                ));
            }
        }
    }
    if ops.is_empty() {
        Ok(())
    } else {
        Err(UnsupportedOps { backend, ops })
    }
}

/// Compile Crush source to native code, load it, and extract an i64 result.
///
/// Convenience function for the common case of calling `main()`.
pub fn eval_i64(source: &str) -> anyhow::Result<i64> {
    let compiler = AotCompiler::new();
    let so_path = compiler.compile_source(source, "eval")?;
    let module = Module::load(so_path)?;
    let result = module.call_main()?;
    result
        .as_int()
        .ok_or_else(|| anyhow::anyhow!("Expected i64, got {:?}", result))
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
    result
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("Expected bool, got {:?}", result))
}
