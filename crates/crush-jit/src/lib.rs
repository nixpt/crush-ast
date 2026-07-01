//! JIT execution engine for Crush VM.
//!
//! [`JitEngine`] compiles [`LoweredProgram`] to native code via Cranelift
//! and executes it in a single shot, returning the same [`FastYield`]
//! that the FastVM interpreter would produce.

pub mod compiler;
pub mod runtime;
pub mod value;

use crush_vm::fastvm::{FastYield, LoweredProgram};
use crush_vm::value::RuntimeValue;
use crate::compiler::JitCompiler;
use crate::runtime::JitContext;
use crate::value::JitValue;

/// JIT execution engine.
pub struct JitEngine {
    compiler: JitCompiler,
}

impl JitEngine {
    /// Create a new JIT engine (compiler initialisation is deferred).
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            compiler: JitCompiler::new()?,
        })
    }

    /// Compile and execute `program`, returning the result.
    pub fn run(&self, program: &LoweredProgram) -> anyhow::Result<FastYield> {
        let compiled = self.compiler.compile(program)?;
        let mut ctx = JitContext::new();
        compiled.execute(&mut ctx);

        if ctx.error != 0 {
            use crush_vm::fastvm::FastError;
            return Ok(FastYield::Error(FastError::ExecutionError(format!(
                "JIT error (flag={})",
                ctx.error
            ))));
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
    } else {
        // Ref / fallback (phase 2)
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
}
