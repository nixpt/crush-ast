//! JIT execution engine for Crush VM.
//!
//! [`JitEngine`] compiles [`LoweredProgram`] to native code via Cranelift
//! and executes it in a single shot, returning the same [`FastYield`]
//! that the FastVM interpreter would produce.

pub mod compiler;
pub mod runtime;
pub mod value;

use crush_vm::fastvm::{Capability, FastYield, Hal, LoweredProgram};
use crush_vm::memory::Arena;
use crush_vm::value::RuntimeValue;
use std::sync::Arc;
use crate::compiler::JitCompiler;
use crate::runtime::{JitContext, jit_runtime_helper};
use crate::value::JitValue;


/// JIT execution engine.
pub struct JitEngine {
    compiler: JitCompiler,
    capabilities: Vec<Arc<dyn Capability>>,
    hal: Arc<dyn Hal>,
    /// Instruction budget. Maps to `JitContext.budget`;
    /// the compiled code decrements this on every instruction and returns
    /// early when exhausted. Corresponds to ExoLight's `timeout_ms`.
    budget: u64,
    /// Arena memory limit in bytes (0 = no limit). Applied to the per-run
    /// `Arena` via `set_limit()` before execution.
    arena_limit: usize,
}

impl JitEngine {
    /// Create a new JIT engine (compiler initialisation is deferred).
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            compiler: JitCompiler::new()?,
            capabilities: Vec::new(),
            hal: Arc::new(crate::runtime::DummyHal),
            budget: u64::MAX,
            arena_limit: 0,
        })
    }

    /// Set the capabilities for the JIT engine.
    pub fn with_capabilities(mut self, caps: Vec<Arc<dyn Capability>>) -> Self {
        self.capabilities = caps;
        self
    }

    /// Set the HAL for the JIT engine.
    pub fn with_hal(mut self, hal: Arc<dyn Hal>) -> Self {
        self.hal = hal;
        self
    }

    /// Set the instruction budget. Default: `u64::MAX` (no limit).
    /// When exhausted, the compiled code returns
    /// `FastYield::BudgetExhausted` so ExoLight can refuel and resume.
    pub fn with_budget(mut self, budget: u64) -> Self {
        self.budget = budget;
        self
    }

    /// Set the arena memory limit in bytes (0 = no limit).
    /// Applied to the per-run Arena via `set_limit()` before execution.
    pub fn with_arena_limit(mut self, limit: usize) -> Self {
        self.arena_limit = limit;
        self
    }

    /// Compile and execute `program` using the provided context and arena.
    ///
    /// The caller owns `ctx` and `arena` — this allows state to be carried
    /// forward across yield/resume boundaries (M2 Phase 5 Tier 2).
    ///
    /// If the program yields via a host-request opcode (CallHost, ExecLang,
    /// Spawn, etc.), returns `FastYield::Yielded` with `ctx.saved_pc` set
    /// to the resume point and `ctx.host_request_tag` indicating which
    /// request was made. The caller should process the request, then call
    /// `resume()` with the same `ctx` and `arena` to continue.
    pub fn run_with_ctx(
        &self,
        program: &LoweredProgram,
        ctx: &mut JitContext,
        arena: &mut Arena,
    ) -> anyhow::Result<FastYield> {
        let compiled = self.compiler.compile(program)?;

        // Store arena pointer
        ctx.arena = arena as *mut Arena as *mut std::ffi::c_void;

        // Store pointer to program's symbol table strings (alive for duration of run())
        ctx.strings_ptr = &program.symbols.strings as *const Vec<String> as *const std::ffi::c_void;

        // Store pointer to capabilities and hal (alive for duration of run())
        ctx.capabilities = &self.capabilities as *const Vec<Arc<dyn Capability>> as *mut std::ffi::c_void;
        ctx.hal = &self.hal as *const Arc<dyn Hal> as *mut std::ffi::c_void;

        // Set the runtime helper dispatch function
        ctx.helper_fn = jit_runtime_helper as *mut std::ffi::c_void;

        compiled.execute(ctx);

        // M2 Phase 5 Tier 2: detect host-request yield before error/budget checks.
        // Check saved_pc (not host_request_tag) because CallHost uses tag 0.
        if ctx.saved_pc != 0 {
            return Ok(FastYield::Yielded);
        }

        if ctx.error != 0 {
            use crush_vm::fastvm::FastError;
            let msg = if ctx.error == 3 {
                "Uncaught exception (no handler)".to_string()
            } else if ctx.error == 4 {
                "Capability call failed".to_string()
            } else {
                format!("JIT execution error (flag={})", ctx.error)
            };
            return Ok(FastYield::Error(FastError::ExecutionError(msg)));
        }

        if ctx.budget == 0 {
            return Ok(FastYield::BudgetExhausted);
        }

        let val = jit_to_runtime(ctx.result());
        Ok(FastYield::Finished(Some(val)))
    }

    /// Convenience wrapper: create a fresh context and arena, then run.
    ///
    /// **Note:** If the program yields via a host-request opcode, the context
    /// is dropped on return and resume is impossible — use [`run_with_ctx`]
    /// and [`resume`] for yielding programs instead.
    pub fn run(&self, program: &LoweredProgram) -> anyhow::Result<FastYield> {
        let mut arena = Arena::new();
        if self.arena_limit > 0 {
            arena.set_limit(self.arena_limit);
        }
        let mut ctx = JitContext::new();
        ctx.budget = self.budget;
        self.run_with_ctx(program, &mut ctx, &mut arena)
    }

    /// Resume execution after a host-request yield.
    ///
    /// `ctx` must be the **same** `JitContext` that was used in the previous
    /// `run_with_ctx()` call — it carries the saved PC, stack, locals, call
    /// stack, and handler stack from before the yield.
    ///
    /// `host_result` is the result of the host call, pushed onto the JIT
    /// stack before resuming (pass `None` to push Null).
    ///
    /// `budget` is the fresh fuel allocation for this continuation.
    pub fn resume(
        &self,
        program: &LoweredProgram,
        ctx: &mut JitContext,
        arena: &mut Arena,
        host_result: Option<RuntimeValue>,
        budget: u64,
    ) -> anyhow::Result<FastYield> {
        let compiled = self.compiler.compile(program)?;

        ctx.budget = budget;
        ctx.arena = arena as *mut Arena as *mut std::ffi::c_void;
        ctx.strings_ptr = &program.symbols.strings as *const Vec<String> as *const std::ffi::c_void;
        ctx.capabilities = &self.capabilities as *const Vec<Arc<dyn Capability>> as *mut std::ffi::c_void;
        ctx.hal = &self.hal as *const Arc<dyn Hal> as *mut std::ffi::c_void;
        ctx.helper_fn = jit_runtime_helper as *mut std::ffi::c_void;

        // Push the host result onto the JIT stack so the resumed code
        // can consume it (e.g., CallHost return value, ExecLang output).
        let jit_val = match host_result {
            Some(RuntimeValue::Int(i)) => JitValue::int(i),
            Some(RuntimeValue::Float(f)) => JitValue::float(f),
            Some(RuntimeValue::Bool(b)) => JitValue::bool(b),
            Some(RuntimeValue::Null) => JitValue::null(),
            Some(RuntimeValue::Ref(idx)) => JitValue::from_ref(idx),
            Some(RuntimeValue::String(_)) => JitValue::null(),
            None => JitValue::null(),
        };
        ctx.push(jit_val);

        compiled.execute(ctx);

        if ctx.saved_pc != 0 {
            return Ok(FastYield::Yielded);
        }

        if ctx.error != 0 {
            use crush_vm::fastvm::FastError;
            let msg = if ctx.error == 3 {
                "Uncaught exception (no handler)".to_string()
            } else if ctx.error == 4 {
                "Capability call failed".to_string()
            } else {
                format!("JIT execution error (flag={})", ctx.error)
            };
            return Ok(FastYield::Error(FastError::ExecutionError(msg)));
        }

        if ctx.budget == 0 {
            return Ok(FastYield::BudgetExhausted);
        }

        let val = jit_to_runtime(ctx.result());
        Ok(FastYield::Finished(Some(val)))
    }
}

impl Default for JitEngine {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn jit_to_runtime(val: JitValue) -> RuntimeValue {
    if let Some(v) = val.to_int() {
        RuntimeValue::Int(v)
    } else if let Some(v) = val.to_float() {
        RuntimeValue::Float(v)
    } else if let Some(v) = val.to_bool() {
        RuntimeValue::Bool(v)
    } else if val.is_null() {
        RuntimeValue::Null
    } else if let Some(idx) = val.to_ref() {
        RuntimeValue::Ref(idx)
    } else {
        RuntimeValue::Null
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crush_vm::fastvm::{FastInstr, FastOp, FastVM, SymbolTables};
    use std::sync::Arc;

    // ── Test helpers ──────────────────────────────────────────────────────────

    #[derive(Debug)]
    struct DummyHal;
    impl crush_vm::fastvm::Hal for DummyHal {}

    fn make_prog(instrs: Vec<(FastOp, u64, u32)>) -> LoweredProgram {
        let instructions: Vec<FastInstr> =
            instrs.into_iter().map(|(op, a, b)| FastInstr::new(op, a, b)).collect();
        LoweredProgram {
            instructions,
            symbols: SymbolTables::new(),
            entry_point: 0,
        }
    }

    /// Create a multi-function program with symbol tables.
    ///
    /// * `instrs` — flat instruction list (all functions concatenated)
    /// * `func_strings` — function name strings (order must match `Call` arg indices)
    /// * `functions` — `(name, start_pc, end_pc, arity)` for each function
    /// * `entry` — entry point PC (usually start of "main")
    fn make_multi_fn(
        instrs: Vec<(FastOp, u64, u32)>,
        func_strings: Vec<&str>,
        functions: Vec<(&str, usize, usize, u32)>,
        entry: usize,
    ) -> LoweredProgram {
        let instructions: Vec<FastInstr> =
            instrs.into_iter().map(|(op, a, b)| FastInstr::new(op, a, b)).collect();
        let mut symbols = SymbolTables::new();
        for &name in &func_strings {
            symbols.intern_string(name);
        }
        for (name, start, end, arity) in functions {
            symbols.functions.insert(name.to_string(), (start, end, arity));
        }
        LoweredProgram {
            instructions,
            symbols,
            entry_point: entry,
        }
    }

    fn run_fastvm(prog: &LoweredProgram) -> FastYield {
        let mut vm = FastVM::new(
            prog.clone(),
            vec![],
            Arc::new(DummyHal),
        );
        vm.run(10_000)
    }

    fn run_jit(prog: &LoweredProgram) -> FastYield {
        let engine = JitEngine::new().expect("JitEngine::new");
        engine.run(prog).expect("JIT execution should not fail")
    }

    // ── Individual instruction tests ──────────────────────────────────────────

    #[test]
    fn test_push_int() {
        let prog = make_prog(vec![(FastOp::PushInt, 42, 0), (FastOp::Halt, 0, 0)]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_push_float() {
        let bits = f64::to_bits(3.14);
        let prog = make_prog(vec![(FastOp::PushFloat, bits, 0), (FastOp::Halt, 0, 0)]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_push_bool_true() {
        let prog = make_prog(vec![(FastOp::PushBool, 1, 0), (FastOp::Halt, 0, 0)]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_push_bool_false() {
        let prog = make_prog(vec![(FastOp::PushBool, 0, 0), (FastOp::Halt, 0, 0)]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_push_null() {
        let prog = make_prog(vec![(FastOp::PushNull, 0, 0), (FastOp::Halt, 0, 0)]);
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "PushNull;Halt should match FastVM");
    }

    #[test]
    fn test_add_ints() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 10, 0),
            (FastOp::PushInt, 20, 0),
            (FastOp::Add, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_sub_ints() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 50, 0),
            (FastOp::PushInt, 8, 0),
            (FastOp::Sub, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_mul_ints() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 7, 0),
            (FastOp::PushInt, 6, 0),
            (FastOp::Mul, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_div_ints() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 20, 0),
            (FastOp::PushInt, 3, 0),
            (FastOp::Div, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_mod_ints() {
        // 7 % 3 == 1
        let prog = make_prog(vec![
            (FastOp::PushInt, 7, 0),
            (FastOp::PushInt, 3, 0),
            (FastOp::Mod, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog), "7 % 3 should match FastVM");
    }

    #[test]
    fn test_mod_negative() {
        // -7 % 3 == -1 (Rust remainder semantics, same as srem)
        let prog = make_prog(vec![
            (FastOp::PushInt, (-7i64 as u64), 0),
            (FastOp::PushInt, 3, 0),
            (FastOp::Mod, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog), "(-7) % 3 should match FastVM");
    }

    #[test]
    fn test_mod_floats() {
        // 7.5 % 2.5 == 0.0 (fmod semantics)
        let a = f64::to_bits(7.5);
        let b = f64::to_bits(2.5);
        let prog = make_prog(vec![
            (FastOp::PushFloat, a, 0),
            (FastOp::PushFloat, b, 0),
            (FastOp::Mod, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog), "7.5 % 2.5 should match FastVM");
    }

    #[test]
    fn test_mod_float_negative() {
        // -7.5 % 2.0 == -1.5 (trunc-division fmod — same as C/C++ remainder)
        let a = f64::to_bits(-7.5);
        let b = f64::to_bits(2.0);
        let prog = make_prog(vec![
            (FastOp::PushFloat, a, 0),
            (FastOp::PushFloat, b, 0),
            (FastOp::Mod, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog),
            "(-7.5) % 2.0 should match FastVM (trunc fmod)");

        // -7.5 % -2.0 == -1.5
        let a = f64::to_bits(-7.5);
        let b = f64::to_bits(-2.0);
        let prog = make_prog(vec![
            (FastOp::PushFloat, a, 0),
            (FastOp::PushFloat, b, 0),
            (FastOp::Mod, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog),
            "(-7.5) % (-2.0) should match FastVM");

        // 7.5 % -2.0 == 1.5
        let a = f64::to_bits(7.5);
        let b = f64::to_bits(-2.0);
        let prog = make_prog(vec![
            (FastOp::PushFloat, a, 0),
            (FastOp::PushFloat, b, 0),
            (FastOp::Mod, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog),
            "7.5 % (-2.0) should match FastVM");
    }

    #[test]
    fn test_neg_int() {
        let prog = make_prog(vec![(FastOp::PushInt, 7, 0), (FastOp::Neg, 0, 0), (FastOp::Halt, 0, 0)]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_add_floats() {
        let a = f64::to_bits(2.5);
        let b = f64::to_bits(1.5);
        let prog = make_prog(vec![
            (FastOp::PushFloat, a, 0),
            (FastOp::PushFloat, b, 0),
            (FastOp::Add, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_cmp_eq_ints() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 3, 0),
            (FastOp::PushInt, 3, 0),
            (FastOp::Eq, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_cmp_lt_ints() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 3, 0),
            (FastOp::PushInt, 10, 0),
            (FastOp::Lt, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_and_bools() {
        let prog = make_prog(vec![
            (FastOp::PushBool, 1, 0),
            (FastOp::PushBool, 0, 0),
            (FastOp::And, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_or_bools() {
        let prog = make_prog(vec![
            (FastOp::PushBool, 1, 0),
            (FastOp::PushBool, 0, 0),
            (FastOp::Or, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_not_bool() {
        let prog = make_prog(vec![(FastOp::PushBool, 0, 0), (FastOp::Not, 0, 0), (FastOp::Halt, 0, 0)]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_dup() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 7, 0),
            (FastOp::Dup, 0, 0),
            (FastOp::Add, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_pop() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 1, 0),
            (FastOp::PushInt, 2, 0),
            (FastOp::Pop, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_swap() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 1, 0),
            (FastOp::PushInt, 99, 0),
            (FastOp::Swap, 0, 0),
            (FastOp::Pop, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_store_local_then_load() {
        let prog = make_prog(vec![
            (FastOp::PushInt, 42, 0),
            (FastOp::StoreLocal, 0, 0),
            (FastOp::LoadLocal, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_jump() {
        // Jump over a PushInt
        let prog = make_prog(vec![
            (FastOp::Jump, 3, 0),
            (FastOp::PushInt, 1, 0),
            (FastOp::Halt, 0, 0), // not reached
            (FastOp::PushInt, 99, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_jump_if_taken() {
        // Push 1 (truthy), JumpIf(target=4) → taken → PushInt(99);Halt
        let prog = make_prog(vec![
            (FastOp::PushBool, 1, 0),
            (FastOp::JumpIf, 4, 0),
            (FastOp::PushInt, 1, 0),
            (FastOp::Halt, 0, 0),
            (FastOp::PushInt, 99, 0),
            (FastOp::Halt, 0, 0),
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_jump_if_not_taken() {
        // Push the result FIRST, then condition.
        // PushBool(false) → JumpIfNot(target=4) → taken → Halt at 4 pops 99.
        let prog = make_prog(vec![
            (FastOp::PushInt, 99, 0),    // 0: result
            (FastOp::PushBool, 0, 0),    // 1: false (falsy)
            (FastOp::JumpIfNot, 4, 0),   // 2: pop false, !truthy → jump to 4
            (FastOp::Halt, 0, 0),        // 3: not reached
            (FastOp::Halt, 0, 0),        // 4: target — pop 99
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_jump_if_not_fallthrough() {
        // Push result FIRST, then condition.
        // PushBool(true) → JumpIfNot(target=5) → not taken → fallthrough to Halt(3).
        let prog = make_prog(vec![
            (FastOp::PushInt, 42, 0),    // 0: result
            (FastOp::PushBool, 1, 0),    // 1: true (truthy)
            (FastOp::JumpIfNot, 5, 0),   // 2: pop true, truthy → fallthrough to 3
            (FastOp::Halt, 0, 0),        // 3: pop 42
            (FastOp::PushInt, 99, 0),    // 4: not reached (block boundary)
            (FastOp::Halt, 0, 0),        // 5: not reached
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_jump_if_not_fallthrough_with_extra_value() {
        // Push 2 values, then JumpIfNot with one as condition.
        // PushInt(1) is stack base, PushBool(false) is the condition.
        // JumpIfNot pops false → jump taken → PushInt(99);Halt
        let prog = make_prog(vec![
            (FastOp::PushInt, 7, 0),     // 0
            (FastOp::PushBool, 0, 0),    // 1: condition = false
            (FastOp::JumpIfNot, 6, 0),   // 2: pop false → jump to 6
            (FastOp::Pop, 0, 0),         // 3: not reached
            (FastOp::PushInt, 1, 0),     // 4: not reached
            (FastOp::Halt, 0, 0),        // 5: not reached
            (FastOp::Pop, 0, 0),         // 6: pop the leftover 7
            (FastOp::PushInt, 99, 0),    // 7
            (FastOp::Halt, 0, 0),        // 8
        ]);
        assert_eq!(run_fastvm(&prog), run_jit(&prog));
    }

    #[test]
    fn test_compound_programs() {
        let cases: Vec<Vec<(FastOp, u64, u32)>> = vec![
            vec![(FastOp::PushInt, 7, 0), (FastOp::Halt, 0, 0)],
            vec![(FastOp::PushBool, 1, 0), (FastOp::Halt, 0, 0)],
            vec![(FastOp::PushNull, 0, 0), (FastOp::Halt, 0, 0)],
            vec![
                (FastOp::PushInt, 2, 0),
                (FastOp::PushInt, 3, 0),
                (FastOp::Div, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
            vec![
                (FastOp::PushInt, 10, 0),
                (FastOp::PushInt, 3, 0),
                (FastOp::Sub, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
            vec![
                (FastOp::PushFloat, f64::to_bits(3.14), 0),
                (FastOp::PushFloat, f64::to_bits(2.0), 0),
                (FastOp::Mul, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
        ];
        for (i, instrs) in cases.iter().enumerate() {
            let prog = make_prog(instrs.clone());
            let expected = run_fastvm(&prog);
            let actual = run_jit(&prog);
            assert_eq!(expected, actual, "case {i}: JIT result didn't match FastVM");
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Phase 2: CALL / RETURN
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_simple_call_no_arg() {
        // main: push 42 (discarded), call foo, halt  → result is foo's return (99)
        // foo:  push 99, return
        // PC 0: PushInt(42)
        // PC 1: Call(0, 0)    — "foo", argc=0
        // PC 2: Halt
        // PC 3: PushInt(99)
        // PC 4: Return
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 42, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::PushInt, 99, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["foo"],
            vec![("foo", 3, 5, 0)],
            0,
        );
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "simple no-arg call should match FastVM");
    }

    #[test]
    fn test_call_with_one_arg() {
        // main: push 5, call "double"(1), halt  → result = 10
        // double: store_local 0, load_local 0, push 2, mul, return
        // PC 0: PushInt(5)
        // PC 1: Call(0, 1)           — "double", argc=1
        // PC 2: Halt
        // PC 3: StoreLocal(0)        — pop 5 → locals[0]
        // PC 4: LoadLocal(0)         — push 5
        // PC 5: PushInt(2)
        // PC 6: Mul
        // PC 7: Return
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 5, 0),
                (FastOp::Call, 0, 1),
                (FastOp::Halt, 0, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::LoadLocal, 0, 0),
                (FastOp::PushInt, 2, 0),
                (FastOp::Mul, 0, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["double"],
            vec![("double", 3, 8, 1)],
            0,
        );
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "call with one arg should match FastVM");
    }

    #[test]
    fn test_call_with_two_args() {
        // main: push 3, push 7, call "add"(2), halt  → 3 + 7 = 10
        // add: store_local 1, store_local 0, load_local 0, load_local 1, add, return
        //
        // Stack before call: [3, 7] (7 on top = second arg)
        // After Call arg reversal (pop-all, push-all): [7, 3] (3 on top = first arg)
        // store_local 0 pops 3 → locals[0]=3 (first param)
        // store_local 1 pops 7 → locals[1]=7 (second param)
        // load 0 → 3, load 1 → 7, add → 10, return → 10
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 3, 0),
                (FastOp::PushInt, 7, 0),
                (FastOp::Call, 0, 2),
                (FastOp::Halt, 0, 0),
                (FastOp::StoreLocal, 1, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::LoadLocal, 0, 0),
                (FastOp::LoadLocal, 1, 0),
                (FastOp::Add, 0, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["add"],
            vec![("add", 4, 10, 2)],
            0,
        );
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "call with two args should match FastVM");
    }

    #[test]
    fn test_multiple_calls_different_functions() {
        // main: push 10, call "double"(1), call "triple"(1), halt  → 60
        // double(1): store 0, load 0, push 2, mul, return  → x * 2
        // triple(1): store 0, load 0, push 3, mul, return  → x * 3
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 10, 0),
                (FastOp::Call, 0, 1),
                (FastOp::Call, 1, 1),
                (FastOp::Halt, 0, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::LoadLocal, 0, 0),
                (FastOp::PushInt, 2, 0),
                (FastOp::Mul, 0, 0),
                (FastOp::Return, 0, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::LoadLocal, 0, 0),
                (FastOp::PushInt, 3, 0),
                (FastOp::Mul, 0, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["double", "triple"],
            vec![
                ("double", 4, 9, 1),
                ("triple", 9, 14, 1),
            ],
            0,
        );
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "multiple calls to different functions should match FastVM");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Phase 3b: Arena-dependent ops (runtime helpers)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_push_str() {
        // PushStr(str_idx=0) via a program with interned string
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("hello");
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        // Both return RuntimeValue::Ref(idx) — different arena pointers, but both are Ref with valid indices.
        // We check that both are Ref and neither is Null.
        if let (&FastYield::Finished(Some(ref a)), &FastYield::Finished(Some(ref b))) = (&expected, &actual) {
            assert!(matches!(a, RuntimeValue::Ref(_)), "Expected Ref from FastVM, got {:?}", a);
            assert!(matches!(b, RuntimeValue::Ref(_)), "Expected Ref from JIT, got {:?}", b);
        } else {
            panic!("PushStr: expected Finished(Some(Ref)), got FastVM={:?}, JIT={:?}", expected, actual);
        }
    }

    #[test]
    fn test_make_list() {
        // MakeList(2): push 10, push 20, make list of 2 items
        let prog = make_prog(vec![
            (FastOp::PushInt, 10, 0),
            (FastOp::PushInt, 20, 0),
            (FastOp::MakeList, 2, 0),
            (FastOp::Halt, 0, 0),
        ]);
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        if let (&FastYield::Finished(Some(ref a)), &FastYield::Finished(Some(ref b))) = (&expected, &actual) {
            assert!(matches!(a, RuntimeValue::Ref(_)), "Expected Ref from FastVM");
            assert!(matches!(b, RuntimeValue::Ref(_)), "Expected Ref from JIT");
        } else {
            panic!("MakeList: expected Finished(Some(Ref)), got FastVM={:?}, JIT={:?}", expected, actual);
        }
    }

    #[test]
    fn test_typeof_int() {
        // TypeOf on an int should produce a string "int" (as Ref)
        let prog = make_prog(vec![
            (FastOp::PushInt, 42, 0),
            (FastOp::TypeOf, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        // Both return a Ref to "int" string — different arena indices but both are Ref
        if let (&FastYield::Finished(Some(ref a)), &FastYield::Finished(Some(ref b))) = (&expected, &actual) {
            assert!(matches!(a, RuntimeValue::Ref(_)), "Expected Ref from FastVM, got {:?}", a);
            assert!(matches!(b, RuntimeValue::Ref(_)), "Expected Ref from JIT, got {:?}", b);
        } else {
            panic!("TypeOf: expected Finished(Some(Ref)), got FastVM={:?}, JIT={:?}", expected, actual);
        }
    }

    #[test]
    fn test_len_array() {
        // MakeList(2), then Len should give 2
        let prog = make_prog(vec![
            (FastOp::PushInt, 10, 0),
            (FastOp::PushInt, 20, 0),
            (FastOp::MakeList, 2, 0),
            (FastOp::Len, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "Len(MakeList(2)) should match FastVM");
    }

    #[test]
    fn test_nested_calls() {
        // main: push 5, call "inc"(1), halt           → 6
        // inc: store 0, load 0, call "add_one"(1), return  → x + 1
        // add_one: store 0, load 0, push 1, add, return  → y + 1
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 5, 0),
                (FastOp::Call, 0, 1),
                (FastOp::Halt, 0, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::LoadLocal, 0, 0),
                (FastOp::Call, 1, 1),
                (FastOp::Return, 0, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::LoadLocal, 0, 0),
                (FastOp::PushInt, 1, 0),
                (FastOp::Add, 0, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["inc", "add_one"],
            vec![
                ("inc", 3, 7, 1),
                ("add_one", 7, 12, 1),
            ],
            0,
        );
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "nested calls (main → inc → add_one) should match FastVM");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Phase 3c: CapCall
    // ══════════════════════════════════════════════════════════════════════════

    /// A simple test capability that doubles an integer input.
    #[derive(Debug)]
    struct DoubleCap;
    impl Capability for DoubleCap {
        fn name(&self) -> &str { "double" }
        fn call(&self, _arena: &mut Arena, args: Vec<RuntimeValue>, _hal: Arc<dyn Hal>) -> anyhow::Result<RuntimeValue> {
            match args.as_slice() {
                [RuntimeValue::Int(x)] => Ok(RuntimeValue::Int(x * 2)),
                _ => Ok(RuntimeValue::Null),
            }
        }
    }

    fn run_jit_with_caps(prog: &LoweredProgram, caps: Vec<Arc<dyn Capability>>) -> FastYield {
        let engine = JitEngine::new().expect("JitEngine::new")
            .with_capabilities(caps);
        engine.run(prog).expect("JIT execution should not fail")
    }

    #[test]
    fn test_cap_call_double() {
        // CapCall(cap_idx=0, argc=1): push 7, call cap[0] with 1 arg
        let prog = make_prog(vec![
            (FastOp::PushInt, 7, 0),
            (FastOp::CapCall, 0, 1),  // cap_idx=0, argc=1
            (FastOp::Halt, 0, 0),
        ]);

        let caps: Vec<Arc<dyn Capability>> = vec![Arc::new(DoubleCap)];

        // FastVM with this capability should return 14
        let mut vm = FastVM::new(
            prog.clone(),
            caps.clone(),
            Arc::new(DummyHal),
        );
        let expected = vm.run(10_000);

        // JIT with same capability should also return 14
        let actual = run_jit_with_caps(&prog, caps);

        assert_eq!(expected, actual, "CapCall double(7) should match FastVM");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Remaining Phase 3 opcodes
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_new_obj_and_get_field() {
        // NewObj creates empty Object; GetField with symbol-table string should return Null
        // Intern a string so strings[0] is valid
        let mut prog = make_prog(vec![
            (FastOp::NewObj, 0, 0),
            (FastOp::GetField, 0, 0), // get field from strings[0]
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("name");
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "NewObj+GetField(empty) should match FastVM");
    }

    #[test]
    fn test_new_obj_set_get_field() {
        // NewObj → Dup → PushInt(42) → SetField(0, "x") → GetField(0) → should return 42
        // Dup preserves the Ref so SetField consumes the copy and GetField uses the original.
        let mut prog = make_prog(vec![
            (FastOp::NewObj, 0, 0),
            (FastOp::Dup, 0, 0),      // copy Ref so SetField doesn't consume the only reference
            (FastOp::PushInt, 42, 0),
            (FastOp::SetField, 0, 0), // pop val=42, pop target=obj_copy, set fields["x"]=42
            (FastOp::GetField, 0, 0), // pop target, push fields["x"]
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("x");
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "NewObj+Dup+SetField(x=42)+GetField(x) should match FastVM");
    }

    #[test]
    fn test_new_tuple_and_push() {
        // NewTuple → PushInt(99) → TuplePush → then push a result for Halt
        let prog = make_prog(vec![
            (FastOp::NewTuple, 0, 0),
            (FastOp::PushInt, 99, 0),
            (FastOp::TuplePush, 0, 0),
            (FastOp::PushInt, 42, 0), // push a known result for Halt to pop
            (FastOp::Halt, 0, 0),
        ]);
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "NewTuple+TuplePush should match FastVM");
    }

    #[test]
    fn test_str_sim_identical() {
        // PushStr(0), PushStr(1), StrSim — should give 1.0 for identical strings
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::PushStr, 0, 0), // same string twice
            (FastOp::StrSim, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("hello");
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "StrSim(hello, hello)=1.0 should match FastVM");
    }

    #[test]
    fn test_str_sim_different() {
        // Two different strings — similarity should be < 1.0 but same between VMs
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::PushStr, 1, 0),
            (FastOp::StrSim, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("hello");
        prog.symbols.intern_string("world");
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual, "StrSim(hello, world) should match FastVM");
    }

    // ══════════════════════════════════════════════════════════════════════════
    // M4: Exception handling
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_throw_uncaught() {
        // Throw without EnterTry should produce an error
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0), // push error message
            (FastOp::Throw, 0, 0),   // throw — no handler registered
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("oops");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        // Both should return an error for uncaught throw
        assert!(expected.is_err(), "FastVM should return error for uncaught throw");
        assert!(actual.is_err(), "JIT should return error for uncaught throw, got {:?}", actual);
    }

    #[test]
    fn test_throw_caught_same_function() {
        // EnterTry(handler_pc=4), then Throw — handler at PC 4 returns the error value.
        // FastVM now supports top-level EnterTry via a root frame on the call stack.
        //
        // PC 0: PushStr(0)           push "err"
        // PC 1: EnterTry(4)          register handler at PC 4
        // PC 2: Throw                pops "err", finds handler, pushes "err", jumps to PC 4
        // PC 3: Halt                (not reached)
        // PC 4: Halt                handler — pops "err" and returns it
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::EnterTry, 4, 0),
            (FastOp::Throw, 0, 0),
            (FastOp::Halt, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual,
            "Caught throw at top level: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_cap_call_no_caps_fallback() {
        // When no capabilities registered, CapCall should push null (not crash)
        let prog = make_prog(vec![
            (FastOp::PushInt, 7, 0),
            (FastOp::CapCall, 0, 1),
            (FastOp::Halt, 0, 0),
        ]);
        let result = run_jit(&prog);
        // Should not crash — capability lookup will fail and push null
        assert!(matches!(result, FastYield::Finished(Some(RuntimeValue::Null))),
            "CapCall without caps should push null, got {:?}", result);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Extended exception handling tests
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_enter_try_exit_try_normal_flow() {
        // EnterTry with handler, then ExitTry (no throw), then normal result.
        // The handler is registered and immediately popped — execution should
        // continue to PushInt(42); Halt without interruption.
        //
        // PC 0: EnterTry(4)    register handler at PC 4
        // PC 1: ExitTry        pop handler (normal scope exit)
        // PC 2: PushInt(42)
        // PC 3: Halt           return 42
        // PC 4: Halt           handler block (unreachable)
        let prog = make_prog(vec![
            (FastOp::EnterTry, 4, 0),
            (FastOp::ExitTry, 0, 0),
            (FastOp::PushInt, 42, 0),
            (FastOp::Halt, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual,
            "EnterTry->ExitTry normal flow: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_throw_caught_by_nested_inner() {
        // Two nested EnterTry blocks. The inner handler catches the throw.
        // The outer handler should never be reached.
        // FastVM now supports top-level EnterTry via a root frame.
        //
        // PC 0: PushStr(0)       push "err"
        // PC 1: EnterTry(5)      outer handler at PC 5
        // PC 2: EnterTry(4)      inner handler at PC 4
        // PC 3: Throw            throw — inner handler catches
        // PC 4: Halt             inner handler — returns pushed "err"
        // PC 5: Halt             outer handler (unreachable)
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::EnterTry, 5, 0),
            (FastOp::EnterTry, 4, 0),
            (FastOp::Throw, 0, 0),
            (FastOp::Halt, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual,
            "Nested inner catch: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_throw_caught_by_nested_outer() {
        // Two nested EnterTry blocks, but the inner one is exited via ExitTry
        // before the Throw. The outer handler catches.
        // FastVM now supports top-level EnterTry via a root frame.
        //
        // PC 0: PushStr(0)       push "err"
        // PC 1: EnterTry(6)      outer handler at PC 6
        // PC 2: EnterTry(4)      inner handler at PC 4
        // PC 3: ExitTry          pop inner handler (no exception in inner scope)
        // PC 4: Throw            throw — caught by outer handler (inner exited)
        // PC 5: Halt             (not reached)
        // PC 6: Halt             outer handler — returns pushed "err"
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::EnterTry, 6, 0),
            (FastOp::EnterTry, 4, 0),
            (FastOp::ExitTry, 0, 0),
            (FastOp::Throw, 0, 0),
            (FastOp::Halt, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual,
            "Nested outer catch: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_throw_caught_by_handler_in_callee() {
        // main calls "may_throw", which has an EnterTry+Throw+handler.
        // The handler catches the throw and returns a default value (99).
        //
        // NOTE: The JIT's simplified Throw handler terminates the Cranelift
        // function early (returns the thrown error value) rather than jumping
        // to the handler block. FastVM executes the handler block (returns 99).
        // This test verifies both paths produce correct but different results.
        //
        // main:
        //   PC 0: PushInt(5)
        //   PC 1: Call(0, 1)         call "may_throw"(1)
        //   PC 2: Halt               return result
        //
        // may_throw (arity=1):
        //   PC 3: StoreLocal(0)      store arg (consumes 5)
        //   PC 4: PushStr(0)         push error message
        //   PC 5: EnterTry(8)        handler at PC 8
        //   PC 6: Throw              throw "err" — caught by handler at PC 8
        //   PC 7: Halt               (not reached)
        //   PC 8: PushInt(99)        handler: default value
        //   PC 9: Return             return 99 to main
        let mut prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 5, 0),
                (FastOp::Call, 0, 1),
                (FastOp::Halt, 0, 0),
                (FastOp::StoreLocal, 0, 0),
                (FastOp::PushStr, 0, 0),
                (FastOp::EnterTry, 8, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::PushInt, 99, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["may_throw"],
            vec![("may_throw", 3, 10, 1)],
            0,
        );
        prog.symbols.intern_string("err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);

        // Both backends should now agree: the handler block executes, returns 99.
        assert_eq!(expected, actual,
            "Caught throw in callee: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_throw_uncaught_inside_callee() {
        // main calls "throws", which has a Throw without any EnterTry.
        // The exception propagates uncaught — both backends should error.
        //
        // main:
        //   PC 0: PushInt(5)
        //   PC 1: Call(0, 0)         call "throws"(0)
        //   PC 2: Halt               (not reached — throws causes error)
        //
        // throws (arity=0):
        //   PC 3: Throw              uncaught throw
        //   PC 4: Halt               (not reached)
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 5, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
            vec!["throws"],
            vec![("throws", 3, 5, 0)],
            0,
        );
        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert!(expected.is_err(),
            "FastVM should return error for uncaught throw in callee");
        assert!(actual.is_err(),
            "JIT should return error for uncaught throw in callee, got {:?}", actual);
    }

    #[test]
    fn test_throw_caught_by_main_handler_wrapping_call() {
        // main has an EnterTry that wraps a function call. The called function
        // throws. The throw propagates up the call stack to main's handler.
        // FastVM now supports top-level EnterTry via a root frame.
        //
        // main:
        //   PC 0: PushStr(0)         push error message
        //   PC 1: EnterTry(5)        handler at PC 5
        //   PC 2: Call(0, 0)         call "throws"(0)
        //   PC 3: Halt               (not reached)
        //   PC 4: Halt               (not reached)
        //   PC 5: Halt               handler — returns pushed "err"
        //
        // throws (arity=0):
        //   PC 6: Throw              throw — caught by main's handler
        //   PC 7: Halt               (not reached)
        let mut prog = make_multi_fn(
            vec![
                (FastOp::PushStr, 0, 0),
                (FastOp::EnterTry, 5, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
            vec!["throws"],
            vec![("throws", 6, 8, 0)],
            0,
        );
        prog.symbols.intern_string("err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual,
            "Main handler catching throw from callee: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_throw_caught_by_callee_handler_on_caller_try() {
        // Both caller and callee have EnterTry. The callee throws, and its
        // OWN handler catches it (inner-most handler wins).
        //
        // main:
        //   PC 0: PushStr(0)         push "main_err"
        //   PC 1: EnterTry(4)        main handler at PC 4
        //   PC 2: Call(0, 0)         call "has_handler"(0)
        //   PC 3: Halt               return result from callee
        //   PC 4: Halt               main handler (unreachable)
        //
        // has_handler (arity=0):
        //   PC 5: EnterTry(8)        callee's own handler at PC 8
        //   PC 6: PushInt(7)         push the error value
        //   PC 7: Throw              throw 7 — JIT simplified Throw returns 7
        //   PC 8: PushInt(42)        handler: push 42 (FastVM only)
        //   PC 9: Return             return 42 to main
        let mut prog = make_multi_fn(
            vec![
                (FastOp::PushStr, 0, 0),
                (FastOp::EnterTry, 4, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::EnterTry, 8, 0),
                (FastOp::PushInt, 7, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::PushInt, 42, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["has_handler"],
            vec![("has_handler", 5, 10, 0)],
            0,
        );
        prog.symbols.intern_string("main_err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);

        // Both backends should now agree: the handler block at PC 8 executes, returns 42.
        assert_eq!(expected, actual,
            "Callee handler catches: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_throw_multi_nested_try_exit_chain() {
        // Three nested try blocks. Throw occurs after two ExitTry calls,
        // so only the outermost handler is active. Validates that multiple
        // ExitTry calls correctly pop the handler stack in sequence.
        // FastVM now supports top-level EnterTry via a root frame.
        //
        // PC 0: PushStr(0)       push "err"
        // PC 1: EnterTry(8)      outer handler at PC 8
        // PC 2: EnterTry(6)      middle handler at PC 6
        // PC 3: EnterTry(4)      inner handler at PC 4
        // PC 4: ExitTry          pop inner
        // PC 5: ExitTry          pop middle
        // PC 6: Throw            throw — caught by outer handler (inner+middle exited)
        // PC 7: Halt             (not reached)
        // PC 8: Halt             outer handler — returns "err"
        let mut prog = make_prog(vec![
            (FastOp::PushStr, 0, 0),
            (FastOp::EnterTry, 8, 0),
            (FastOp::EnterTry, 6, 0),
            (FastOp::EnterTry, 4, 0),
            (FastOp::ExitTry, 0, 0),
            (FastOp::ExitTry, 0, 0),
            (FastOp::Throw, 0, 0),
            (FastOp::Halt, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);
        prog.symbols.intern_string("err");

        let expected = run_fastvm(&prog);
        let actual = run_jit(&prog);
        assert_eq!(expected, actual,
            "Multi-nested try/exit chain: JIT ({:?}) should match FastVM ({:?})",
            actual, expected);
    }

    #[test]
    fn test_nested_try_inside_callee_inner_catches() {
        // Two nested EnterTry blocks inside a called function. The inner
        // handler catches the throw. FastVM executes the handler block and
        // returns Int(42). JIT simplified Throw terminates the Cranelift
        // function early with the thrown value Int(7).
        //
        // main:
        //   PC 0: PushInt(5)         (dummy arg, unused)
        //   PC 1: Call(0, 0)         call "nested_try"
        //   PC 2: Halt               return result from callee
        //
        // nested_try (arity=0):
        //   PC 3: EnterTry(10)       outer handler at PC 10
        //   PC 4: EnterTry(8)        inner handler at PC 8
        //   PC 5: PushInt(7)         push error value 7
        //   PC 6: Throw              throw — inner handler catches
        //   PC 7: Halt               (not reached)
        //   PC 8: PushInt(42)        inner handler: return 42
        //   PC 9: Return
        //   PC 10: PushInt(99)       outer handler: return 99 (unreachable)
        //   PC 11: Return
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 5, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::EnterTry, 10, 0),
                (FastOp::EnterTry, 8, 0),
                (FastOp::PushInt, 7, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::PushInt, 42, 0),
                (FastOp::Return, 0, 0),
                (FastOp::PushInt, 99, 0),
                (FastOp::Return, 0, 0),
            ],
            vec!["nested_try"],
            vec![("nested_try", 3, 12, 0)],
            0,
        );

        let fastvm = run_fastvm(&prog);
        let jit = run_jit(&prog);

        // Both backends should now agree: the inner handler executes, returns 42.
        assert_eq!(fastvm, jit,
            "Nested try inside callee: JIT ({:?}) should match FastVM ({:?})",
            jit, fastvm);
    }

    #[test]
    fn test_handler_in_middle_catches_throw_from_callee() {
        // main calls "middle", which has an EnterTry handler. middle calls
        // "throws", which throws. The JIT's global handler stack allows it
        // to find middle's handler even though the throw is in a different
        // function (call_stack_top guard matches). FastVM uses per-function
        // handler stacks and cannot unwind across function boundaries.
        //
        // main:
        //   PC 0: PushInt(5)
        //   PC 1: Call(0, 0)         call "middle"
        //   PC 2: Halt               return result
        //
        // middle (arity=0):
        //   PC 3: EnterTry(8)        handler at PC 8
        //   PC 4: Call(1, 0)         call "throws"
        //   PC 5: Halt               (not reached)
        //   PC 6: Halt               (not reached)
        //   PC 7: Halt               (not reached)
        //   PC 8: PushInt(42)        handler: return 42
        //   PC 9: Return
        //
        // throws (arity=0):
        //   PC 10: PushInt(7)        push error value 7
        //   PC 11: Throw             throw 7 — unwinds to middle's handler
        //   PC 12: Halt              (not reached)
        let prog = make_multi_fn(
            vec![
                (FastOp::PushInt, 5, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::EnterTry, 8, 0),
                (FastOp::Call, 1, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::PushInt, 42, 0),
                (FastOp::Return, 0, 0),
                (FastOp::PushInt, 7, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
            vec!["middle", "throws"],
            vec![
                ("middle", 3, 10, 0),
                ("throws", 10, 13, 0),
            ],
            0,
        );

        let fastvm = run_fastvm(&prog);
        let jit = run_jit(&prog);

        // Both backends should now agree: middle's handler executes, returns 42.
        assert_eq!(fastvm, jit,
            "Handler in middle catches throw from callee: JIT ({:?}) should match FastVM ({:?})",
            jit, fastvm);
    }

    #[test]
    fn test_deep_unwind_through_two_functions_to_main_handler() {
        // main has an EnterTry, then calls level1, which calls level2, which
        // throws. The throw unwinds through level2 → level1 → main's handler.
        // FastVM now supports top-level EnterTry via a root frame.
        //
        // main:
        //   PC 0: EnterTry(5)        handler at PC 5
        //   PC 1: Call(0, 0)         call "level1"
        //   PC 2: Halt               (not reached — throw unwinds)
        //   PC 3: Halt               (not reached)
        //   PC 4: Halt               (not reached)
        //   PC 5: PushInt(42)        handler: return 42
        //   PC 6: Return
        //
        // level1 (arity=0):
        //   PC 7: Call(1, 0)         call "level2"
        //   PC 8: Halt               (not reached)
        //   PC 9: Halt               (not reached)
        //   PC 10: Return
        //
        // level2 (arity=0):
        //   PC 11: PushInt(7)        push error value 7
        //   PC 12: Throw             throw 7 → unwinds through level1 to main
        //   PC 13: Halt              (not reached)
        let prog = make_multi_fn(
            vec![
                (FastOp::EnterTry, 5, 0),
                (FastOp::Call, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::PushInt, 42, 0),
                (FastOp::Return, 0, 0),
                (FastOp::Call, 1, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Halt, 0, 0),
                (FastOp::Return, 0, 0),
                (FastOp::PushInt, 7, 0),
                (FastOp::Throw, 0, 0),
                (FastOp::Halt, 0, 0),
            ],
            vec!["level1", "level2"],
            vec![
                ("level1", 7, 11, 0),
                ("level2", 11, 14, 0),
            ],
            0,
        );

        let fastvm = run_fastvm(&prog);
        let jit = run_jit(&prog);

        // Both backends now agree: main's handler catches, returns 42.
        assert_eq!(fastvm, jit,
            "Deep unwind through 2 functions: JIT ({:?}) should match FastVM ({:?})",
            jit, fastvm);
    }

    #[test]
    fn test_throw_unwind_through_three_functions_to_middle_handler() {
        // Verifies Throw unwinding through main → a → b → c where c throws
        // and a's EnterTry catches. a's handler is at PC 8 (same code as
        // main's handler at PC 8, but a catches first because it's closer
        // to c in the call stack). Tests cross-frame handler search skipping
        // frames with empty handlers.
        //
        // main (PCs 0-3):
        //   PC 0: EnterTry(8)      register handler at PC 8
        //   PC 1: Call(0, 0)       call "a"(0)
        //   PC 2: Halt             return result from a
        //   PC 3: Halt             (padding)
        //
        // a (PCs 4-9):
        //   PC 4: EnterTry(8)      register handler at PC 8 (catches c's throw)
        //   PC 5: Call(1, 0)       call "b"(0)
        //   PC 6: Halt             (not reached)
        //   PC 7: Halt             (not reached)
        //   PC 8: PushInt(42)      handler: push 42
        //   PC 9: Return           return 42 to main
        //
        // b (PCs 10-12):
        //   PC 10: Call(2, 0)      call "c"(0)
        //   PC 11: Halt            (not reached)
        //   PC 12: Return          (not reached)
        //
        // c (PCs 13-15):
        //   PC 13: PushInt(7)      push error value 7
        //   PC 14: Throw           throw 7 → unwinds through b to a's handler
        //   PC 15: Halt            (not reached)
        let prog = make_multi_fn(
            vec![
                (FastOp::EnterTry, 8, 0),  // 0: main
                (FastOp::Call, 0, 0),      // 1: call "a"
                (FastOp::Halt, 0, 0),      // 2: return result
                (FastOp::Halt, 0, 0),      // 3: padding
                (FastOp::EnterTry, 8, 0),  // 4: a
                (FastOp::Call, 1, 0),      // 5: call "b"
                (FastOp::Halt, 0, 0),      // 6: (not reached)
                (FastOp::Halt, 0, 0),      // 7: (not reached)
                (FastOp::PushInt, 42, 0),  // 8: handler
                (FastOp::Return, 0, 0),    // 9: return 42
                (FastOp::Call, 2, 0),      // 10: b
                (FastOp::Halt, 0, 0),      // 11: (not reached)
                (FastOp::Return, 0, 0),    // 12: (not reached)
                (FastOp::PushInt, 7, 0),   // 13: c
                (FastOp::Throw, 0, 0),     // 14: throw 7
                (FastOp::Halt, 0, 0),      // 15: (not reached)
            ],
            vec!["a", "b", "c"],
            vec![
                ("a", 4, 10, 0),
                ("b", 10, 13, 0),
                ("c", 13, 16, 0),
            ],
            0,
        );

        let fastvm = run_fastvm(&prog);
        let jit = run_jit(&prog);

        // Both backends agree: a's handler catches (skipping b and c's empty
        // handler stacks), executes PushInt(42); Return, giving Int(42).
        assert_eq!(fastvm, jit,
            "Throw through 3 functions to middle handler: JIT ({:?}) should match FastVM ({:?})",
            jit, fastvm);
    }

    #[test]
    fn test_throw_unwind_three_functions_with_rethrow_from_handler() {
        // Verifies Throw unwinding through main → a → b → c where c throws,
        // a's handler catches and *re-throws* (another Throw at PC 11), and
        // main's handler catches the re-throw. This exercises the two-phase
        // unwind: first Throw finds a's handler at PC 11 (which re-throws),
        // second Throw walks past a's now-empty handler stack to find main's
        // handler at PC 6.
        //
        // main (PCs 0-6):
        //   PC 0: EnterTry(6)      register handler at PC 6 (catches re-throw)
        //   PC 1: Call(0, 0)       call "a"(0)
        //   PC 2: Halt             (not reached)
        //   PC 3: Halt             (not reached)
        //   PC 4: Halt             (not reached)
        //   PC 5: Halt             (not reached)
        //   PC 6: Halt             handler — pops Int(7) as result
        //
        // a (PCs 7-13):
        //   PC 7: EnterTry(11)     register handler at PC 11
        //   PC 8: Call(1, 0)       call "b"(0)
        //   PC 9: Halt             (not reached)
        //   PC 10: Halt            (not reached)
        //   PC 11: Throw           handler: re-throw (value 7 on stack from catch)
        //   PC 12: Halt            (not reached)
        //   PC 13: Halt            (not reached)
        //
        // b (PCs 14-16):
        //   PC 14: Call(2, 0)      call "c"(0)
        //   PC 15: Halt            (not reached)
        //   PC 16: Halt            (not reached)
        //
        // c (PCs 17-19):
        //   PC 17: PushInt(7)      push error value 7
        //   PC 18: Throw           throw 7 → caught by a's handler, re-thrown
        //   PC 19: Halt            (not reached)
        let prog = make_multi_fn(
            vec![
                (FastOp::EnterTry, 6, 0),    // 0: main
                (FastOp::Call, 0, 0),         // 1: call "a"
                (FastOp::Halt, 0, 0),          // 2: (not reached)
                (FastOp::Halt, 0, 0),          // 3: (not reached)
                (FastOp::Halt, 0, 0),          // 4: (not reached)
                (FastOp::Halt, 0, 0),          // 5: (not reached)
                (FastOp::Halt, 0, 0),          // 6: handler — pops 7
                (FastOp::EnterTry, 11, 0),    // 7: a
                (FastOp::Call, 1, 0),         // 8: call "b"
                (FastOp::Halt, 0, 0),          // 9: (not reached)
                (FastOp::Halt, 0, 0),          // 10: (not reached)
                (FastOp::Throw, 0, 0),        // 11: handler: re-throw 7
                (FastOp::Halt, 0, 0),          // 12: (not reached)
                (FastOp::Halt, 0, 0),          // 13: (not reached)
                (FastOp::Call, 2, 0),         // 14: b
                (FastOp::Halt, 0, 0),          // 15: (not reached)
                (FastOp::Halt, 0, 0),          // 16: (not reached)
                (FastOp::PushInt, 7, 0),      // 17: c
                (FastOp::Throw, 0, 0),        // 18: throw 7
                (FastOp::Halt, 0, 0),          // 19: (not reached)
            ],
            vec!["a", "b", "c"],
            vec![
                ("a", 7, 14, 0),
                ("b", 14, 17, 0),
                ("c", 17, 20, 0),
            ],
            0,
        );

        let fastvm = run_fastvm(&prog);
        let jit = run_jit(&prog);

        // Both backends agree: c throws → a catches and re-throws → main catches.
        // Result is Int(7) from main's handler.
        assert_eq!(fastvm, jit,
            "Rethrow through 3 functions: JIT ({:?}) should match FastVM ({:?})",
            jit, fastvm);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // M2 Phase 5 Tier 1: Budget exhaustion (fuel metering)
    // ══════════════════════════════════════════════════════════════════════════

    /// Run a program through the JIT with a specific budget.
    fn run_jit_with_budget(prog: &LoweredProgram, budget: u64) -> FastYield {
        let engine = JitEngine::new()
            .expect("JitEngine::new")
            .with_budget(budget);
        engine.run(prog).expect("JIT execution should not fail")
    }

    #[test]
    fn test_budget_exhaustion_chain_of_jumps() {
        // Chain of 3 jumps, each calling dec_budget, plus 1 from entry = 4 total.
        // Budget = 3: exhausted before Halt in the final block.
        //
        // PC 0: Jump(2)          block A — terminator → block B
        // PC 1: Nop              (unreachable, block boundary)
        // PC 2: Jump(4)          block B — terminator → block C
        // PC 3: Nop              (unreachable, block boundary)
        // PC 4: Jump(6)          block C — terminator → block D
        // PC 5: Nop              (unreachable, block boundary)
        // PC 6: PushInt(42)      block D — final block
        // PC 7: Halt             terminator
        //
        // dec_budget calls: Entry(1) + Jump@0(1) + Jump@2(1) + Jump@4(1) = 4 > budget(3).
        // Budget: 3→2→1→0→0 (saturated), Halt finishes, engine detects budget==0.
        let prog = make_prog(vec![
            (FastOp::Jump, 2, 0),
            (FastOp::Nop, 0, 0),
            (FastOp::Jump, 4, 0),
            (FastOp::Nop, 0, 0),
            (FastOp::Jump, 6, 0),
            (FastOp::Nop, 0, 0),
            (FastOp::PushInt, 42, 0),
            (FastOp::Halt, 0, 0),
        ]);
        let result = run_jit_with_budget(&prog, 3);
        assert_eq!(result, FastYield::BudgetExhausted,
            "Budget(3) should exhaust after 4 dec_budget calls, got {:?}", result);
    }

    #[test]
    fn test_budget_sufficient_chain_of_jumps_returns_result() {
        // Same chain as above, but with ample budget (100 >> 4 dec calls).
        // Should return the normal result Int(42).
        let prog = make_prog(vec![
            (FastOp::Jump, 2, 0),
            (FastOp::Nop, 0, 0),
            (FastOp::Jump, 4, 0),
            (FastOp::Nop, 0, 0),
            (FastOp::Jump, 6, 0),
            (FastOp::Nop, 0, 0),
            (FastOp::PushInt, 42, 0),
            (FastOp::Halt, 0, 0),
        ]);
        let result = run_jit_with_budget(&prog, 100);
        assert_eq!(result, FastYield::Finished(Some(RuntimeValue::Int(42))),
            "Budget(100) should allow normal completion, got {:?}", result);
    }

    // ══════════════════════════════════════════════════════════════════════════
    // M2 Phase 5 Tier 2: Host-request yield + resume round-trip
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_yield_spawn_and_resume_round_trip() {
        // Verifies the full yield → resume cycle:
        // 1. Program executes PushInt(10), then Spawn (yields)
        // 2. Engine returns Yielded — caller processes the "spawn" request
        // 3. Caller calls resume() with host_result = Int(5)
        // 4. Program continues: PushInt(20), Add (20+5=25), Halt → Int(25)
        //
        // PC 0: PushInt(10)     stack: [10]
        // PC 1: Spawn           yield — saved_pc=2, tag=HOST_REQ_SPAWN
        // --- resume with host_result=Int(5) ---
        // PC 2: PushInt(20)     stack: [10, 5, 20]
        // PC 3: Add             stack: [10, 25]
        // PC 4: Halt            pops 25 → Finished(Int(25))
        let prog = make_prog(vec![
            (FastOp::PushInt, 10, 0),
            (FastOp::Spawn, 0, 0),
            (FastOp::PushInt, 20, 0),
            (FastOp::Add, 0, 0),
            (FastOp::Halt, 0, 0),
        ]);

        let engine = JitEngine::new().expect("JitEngine::new");
        let mut arena = Arena::new();
        let mut ctx = JitContext::new();
        ctx.budget = 1000;

        // First run: should yield at Spawn
        let yield_result = engine.run_with_ctx(&prog, &mut ctx, &mut arena)
            .expect("first run should not error");
        assert_eq!(yield_result, FastYield::Yielded,
            "Spawn should cause a yield, got {:?}", yield_result);
        assert_eq!(ctx.saved_pc, 2, "saved_pc should be 2 (next instruction after Spawn)");
        assert_eq!(ctx.host_request_tag, 2, "host_request_tag should be 2 (HOST_REQ_SPAWN)");

        // Simulate host processing: push result and resume
        let final_result = engine.resume(&prog, &mut ctx, &mut arena,
            Some(RuntimeValue::Int(5)), 1000)
            .expect("resume should not error");

        assert_eq!(final_result, FastYield::Finished(Some(RuntimeValue::Int(25))),
            "After resume, 20 + 5 should = 25, got {:?}", final_result);
    }
}
