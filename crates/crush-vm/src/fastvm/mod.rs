//! FastVM: High-performance executor for lowered CASM.
//!
//! This is the "hot path" VM—no strings, no HashMap lookups, no debugger.
//! All name resolution happens at load time via the lowering pass.

pub mod types;
pub mod instructions;
pub mod operations;
pub mod arithmetic;
pub mod similarity;
pub mod execution;

pub use types::{FastYield, HostRequest, FastError, FastFrame, ROOT_FRAME_PC};
pub use instructions::{FastInstr, FastOp, LoweredProgram, SymbolTables, HostCallSite, InterfaceCallSite, ExecLangSite, LowerError, lower_program, lower_bytecode_program};

use crate::{Arena, RuntimeValue};
use std::sync::Arc;

pub trait Hal: Send + Sync + std::fmt::Debug {}

pub trait Capability: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn call(&self, arena: &mut Arena, args: Vec<RuntimeValue>, hal: Arc<dyn Hal>) -> anyhow::Result<RuntimeValue>;
}

/// High-performance VM with index-based execution
pub struct FastVM {
    /// Instruction stream
    pub instructions: Vec<FastInstr>,
    /// Program counter
    pub pc: usize,
    /// Value stack
    pub stack: Vec<RuntimeValue>,
    /// Local variables (flat, indexed by frame base + offset)
    pub locals: Vec<RuntimeValue>,
    /// Call stack
    pub call_stack: Vec<FastFrame>,
    /// Symbol tables for resolution
    pub symbols: SymbolTables,
    /// Pre-resolved capability table
    pub capabilities: Vec<Arc<dyn Capability>>,
    /// HAL for capability calls
    pub hal: Arc<dyn Hal>,
    /// Arena for heap allocations
    pub arena: Arena,
    /// AI-powered optimizer for GC prediction
    pub optimizer: crate::ai_optimizer::VmOptimizer,
    /// Instructions executed since last GC
    pub instructions_since_gc: usize,
}

impl FastVM {
    /// Create a new FastVM from a lowered program
    pub fn new(
        program: LoweredProgram, 
        capabilities: Vec<Arc<dyn Capability>>,
        hal: Arc<dyn Hal>
    ) -> Self {
        Self {
            instructions: program.instructions,
            pc: program.entry_point,
            stack: Vec::with_capacity(256),
            locals: Vec::with_capacity(64),
            // Push a root frame so EnterTry at the top level has a frame
            // on which to register handlers, matching JIT behavior.
            call_stack: vec![FastFrame {
                return_pc: ROOT_FRAME_PC,
                locals_base: 0,
                locals_count: 0,
                handlers: Vec::new(),
            }],
            symbols: program.symbols,
            capabilities,
            hal,
            arena: Arena::new(),
            optimizer: crate::ai_optimizer::VmOptimizer::new(),
            instructions_since_gc: 0,
        }
    }

    /// Get the arena for external access
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Get mutable arena
    pub fn arena_mut(&mut self) -> &mut Arena {
        &mut self.arena
    }

    /// Push a value onto the stack (used for resuming after HostRequest)
    pub fn push_value(&mut self, val: RuntimeValue) {
        self.stack.push(val);
    }

    /// Pop a value from the stack
    pub fn pop_value(&mut self) -> Option<RuntimeValue> {
        self.stack.pop()
    }

    /// Reset VM state and point to a new entry function
    pub fn reset_to_entry(&mut self, entry_name: &str) -> Result<(), String> {
        if let Some(&(start_pc, _end_pc, arity)) = self.symbols.functions.get(entry_name) {
            self.pc = start_pc;
            self.stack.clear();
            self.locals.clear();
            self.call_stack.clear();

            // Push root frame with sentinel return_pc so top-level EnterTry works.
            self.call_stack.push(FastFrame {
                return_pc: ROOT_FRAME_PC,
                locals_base: 0,
                locals_count: arity as usize,
                handlers: Vec::new(),
            });
            
            // Initialize locals with nulls for arity if needed
            self.locals.resize(arity as usize, RuntimeValue::Null);
            
            Ok(())
        } else {
            Err(format!("Function {} not found in symbol table", entry_name))
        }
    }

    /// Run for up to `budget` instructions
    #[inline]
    pub fn run(&mut self, budget: u32) -> FastYield {
        for _ in 0..budget {
            if self.pc >= self.instructions.len() {
                let result = self.stack.pop();
                return FastYield::Finished(result);
            }

            // SAFETY: bounds checked above
            let instr = self.instructions[self.pc];
            self.pc += 1;

            match execution::execute_one(
                instr,
                &mut self.pc,
                &mut self.stack,
                &mut self.locals,
                &mut self.call_stack,
                &self.symbols,
                &self.capabilities,
                &self.hal,
                &mut self.arena,
            ) {
                Ok(None) => {},
                Ok(Some(yield_reason)) => return yield_reason,
                Err(e) => return FastYield::Error(e),
            }

            self.instructions_since_gc += 1;
            // Periodically check AI model if we should GC
            if self.instructions_since_gc % 64 == 0 {
                let current_mem = self.arena.get_memory_usage() as f32;
                let peak_mem = self.arena.stats().peak_usage as f32;
                let alloc_rate = 0.0; // Heuristic
                
                let inputs = vec![current_mem, peak_mem, self.instructions_since_gc as f32, alloc_rate];
                if self.optimizer.should_gc(inputs) {
                    self.collect_garbage();
                }
            }
        }
        
        FastYield::BudgetExhausted
    }

    /// Perform a full GC cycle, tracing from VM stack and locals
    pub fn collect_garbage(&mut self) {
        let mut roots = Vec::new();
        // Trace stack
        for val in &self.stack {
            if let RuntimeValue::Ref(idx) = val {
                roots.push(*idx);
            }
        }
        // Trace locals
        for val in &self.locals {
            if let RuntimeValue::Ref(idx) = val {
                roots.push(*idx);
            }
        }
        
        self.arena.trace(roots);
        let _freed = self.arena.sweep();
        self.instructions_since_gc = 0;
    }

    /// Calculate string similarity using bit-parallel Levenshtein distance (normalized to 0.0 - 1.0)
    pub fn calculate_similarity(&self, s1: &str, s2: &str) -> f64 {
        similarity::calculate_similarity(s1, s2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fastvm::{FastInstr, FastOp, LoweredProgram, SymbolTables};

    #[derive(Debug)]
    struct DummyHal;
    impl super::Hal for DummyHal {}
    use std::sync::Arc;

    fn make_simple_program(instrs: Vec<FastInstr>) -> LoweredProgram {
        LoweredProgram {
            instructions: instrs,
            symbols: SymbolTables::new(),
            entry_point: 0,
        }
    }

    fn make_vm(program: LoweredProgram) -> FastVM {
        FastVM::new(program, vec![], Arc::new(DummyHal))
    }

    /// Create a multi-function lowered program for testing.
    ///
    /// * `instrs` — flat instruction list (all functions concatenated)
    /// * `func_names` — function names in the order their string indices will be interned
    /// * `functions` — `(name, start_pc, end_pc, arity)` for each callable function
    /// * `entry` — entry point PC (usually 0 for main)
    fn make_multi_fn(
        instrs: Vec<FastInstr>,
        func_names: Vec<&str>,
        functions: Vec<(&str, usize, usize, u32)>,
        entry: usize,
    ) -> LoweredProgram {
        let mut symbols = SymbolTables::new();
        for &name in &func_names {
            symbols.intern_string(name);
        }
        for (name, start, end, arity) in functions {
            symbols.functions.insert(name.to_string(), (start, end, arity));
        }
        LoweredProgram {
            instructions: instrs,
            symbols,
            entry_point: entry,
        }
    }

    #[test]
    fn test_push_and_add() {
        let program = make_simple_program(vec![
            FastInstr::new(FastOp::PushInt, 10, 0),
            FastInstr::new(FastOp::PushInt, 20, 0),
            FastInstr::simple(FastOp::Add),
            FastInstr::simple(FastOp::Halt),
        ]);
        
        let mut vm = make_vm(program);
        let result = vm.run(100);
        
        assert_eq!(result, FastYield::Finished(Some(RuntimeValue::Int(30))));
    }

    #[test]
    fn test_comparison() {
        let program = make_simple_program(vec![
            FastInstr::new(FastOp::PushInt, 10, 0),
            FastInstr::new(FastOp::PushInt, 20, 0),
            FastInstr::simple(FastOp::Lt),
            FastInstr::simple(FastOp::Halt),
        ]);
        
        let mut vm = make_vm(program);
        let result = vm.run(100);
        
        assert_eq!(result, FastYield::Finished(Some(RuntimeValue::Bool(true))));
    }

    #[test]
    fn test_locals() {
        let program = make_simple_program(vec![
            FastInstr::new(FastOp::PushInt, 42, 0),
            FastInstr::new(FastOp::StoreLocal, 0, 0),
            FastInstr::new(FastOp::LoadLocal, 0, 0),
            FastInstr::simple(FastOp::Halt),
        ]);
        
        let mut vm = make_vm(program);
        let result = vm.run(100);
        
        assert_eq!(result, FastYield::Finished(Some(RuntimeValue::Int(42))));
    }

    #[test]
    fn test_budget_exhaustion() {
        let program = make_simple_program(vec![
            FastInstr::new(FastOp::PushInt, 1, 0),
            FastInstr::new(FastOp::PushInt, 1, 0),
            FastInstr::simple(FastOp::Add),
            FastInstr::new(FastOp::Jump, 0, 0), // Infinite loop
        ]);
        
        let mut vm = make_vm(program);
        let result = vm.run(10);
        
        assert_eq!(result, FastYield::BudgetExhausted);
    }

    #[test]
    fn test_throw_unwind_through_three_functions() {
        // Verifies Throw unwinding through main → a → b → c where c throws
        // and a's EnterTry catches. This exercises cross-frame handler search
        // (skipping frames with empty handlers), frame truncation on catch,
        // and correct return through the remaining call stack.
        //
        // Instruction layout:
        //
        // main (PCs 0-3):
        //   PC 0: EnterTry(8)      register handler at PC 8 (a's handler code)
        //   PC 1: Call(0, 0)       call "a"(0) — string index 0
        //   PC 2: Halt             return result from a (expected: 42)
        //   PC 3: Halt             (unused padding — handler points to PC 8)
        //
        // a (PCs 4-9):
        //   PC 4: EnterTry(8)      register handler at PC 8 (catches c's throw)
        //   PC 5: Call(1, 0)       call "b"(0) — string index 1
        //   PC 6: Halt             (not reached — c throws)
        //   PC 7: Halt             (not reached)
        //   PC 8: PushInt(42)      handler: push 42
        //   PC 9: Return           return 42 to main
        //
        // b (PCs 10-12):
        //   PC 10: Call(2, 0)      call "c"(0) — string index 2
        //   PC 11: Halt            (not reached — c throws)
        //   PC 12: Return          (not reached)
        //
        // c (PCs 13-15):
        //   PC 13: PushInt(7)      push error value 7
        //   PC 14: Throw           throw 7 → unwinds through b to a's handler
        //   PC 15: Halt            (not reached)
        let program = make_multi_fn(
            vec![
                FastInstr::new(FastOp::EnterTry, 8, 0),  // 0: main
                FastInstr::new(FastOp::Call, 0, 0),      // 1: call "a"
                FastInstr::simple(FastOp::Halt),          // 2: return result
                FastInstr::simple(FastOp::Halt),          // 3: padding
                FastInstr::new(FastOp::EnterTry, 8, 0),  // 4: a
                FastInstr::new(FastOp::Call, 1, 0),      // 5: call "b"
                FastInstr::simple(FastOp::Halt),          // 6: (not reached)
                FastInstr::simple(FastOp::Halt),          // 7: (not reached)
                FastInstr::new(FastOp::PushInt, 42, 0),  // 8: handler
                FastInstr::simple(FastOp::Return),        // 9: return 42
                FastInstr::new(FastOp::Call, 2, 0),      // 10: b
                FastInstr::simple(FastOp::Halt),          // 11: (not reached)
                FastInstr::simple(FastOp::Return),        // 12: (not reached)
                FastInstr::new(FastOp::PushInt, 7, 0),   // 13: c
                FastInstr::simple(FastOp::Throw),         // 14: throw 7
                FastInstr::simple(FastOp::Halt),          // 15: (not reached)
            ],
            vec!["a", "b", "c"],
            vec![
                ("a", 4, 10, 0),
                ("b", 10, 13, 0),
                ("c", 13, 16, 0),
            ],
            0,
        );

        let mut vm = make_vm(program);
        let result = vm.run(10_000);

        // a's handler catches and returns 42. main's handler at PC 8 is never
        // reached because a's frame (lower in call stack) catches first.
        assert_eq!(result, FastYield::Finished(Some(RuntimeValue::Int(42))),
            "Throw through 3 functions should unwind to a's handler and return 42, got {:?}",
            result);
    }

    #[test]
    fn test_throw_unwinds_from_callee_to_caller_handler() {
        // Verifies that Throw from a called function unwinds the call stack
        // to a handler in the caller's frame. main has EnterTry, calls
        // "throws" which throws without its own handler.
        //
        // main (PCs 0-5):
        //   PC 0: PushStr(0)       push "err" (reused after throw)
        //   PC 1: EnterTry(5)      register handler at PC 5
        //   PC 2: Call(0, 0)       call "throws"(0) — string index 0
        //   PC 3: Halt             (not reached — throws throws)
        //   PC 4: Halt             (not reached)
        //   PC 5: Halt             handler — returns "err"
        //
        // throws (PCs 6-8):
        //   PC 6: Throw            throw — caught by main's handler
        //   PC 7: Halt             (not reached)
        //   PC 8: Halt             (not reached)
        let mut program = make_multi_fn(
            vec![
                FastInstr::new(FastOp::PushStr, 1, 0),    // 0: push "err" (strings[1] after func names)
                FastInstr::new(FastOp::EnterTry, 5, 0),   // 1: main handler at PC 5
                FastInstr::new(FastOp::Call, 0, 0),        // 2: call "throws"
                FastInstr::simple(FastOp::Halt),           // 3: (not reached)
                FastInstr::simple(FastOp::Halt),           // 4: (not reached)
                FastInstr::simple(FastOp::Halt),           // 5: handler — pops "err" as result
                FastInstr::simple(FastOp::Throw),           // 6: throws — caught by main
                FastInstr::simple(FastOp::Halt),            // 7: (not reached)
                FastInstr::simple(FastOp::Halt),            // 8: (not reached)
            ],
            vec!["throws"],
            vec![("throws", 6, 9, 0)],
            0,
        );
        program.symbols.intern_string("err");

        let mut vm = make_vm(program);
        let result = vm.run(10_000);

        // The handler at PC 5 executes Halt, which pops "err" from the stack.
        // The string was pushed by PushStr(0), and Throw re-pushes it onto the
        // handler's stack, so Halt returns Ref to "err".
        match &result {
            FastYield::Finished(Some(RuntimeValue::Ref(_))) => {} // OK
            other => panic!(
                "Throw from callee caught by main handler: expected Finished(Some(Ref)), got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_throw_unwind_three_functions_uncaught() {
        // Verifies that Throw without any handler produces an error.
        // main → a → b → c where c throws and NO frame has a handler.
        //
        // main (PC 0-2):
        //   PC 0: Call(0, 0)       call "a"(0)
        //   PC 1: Halt             (not reached)
        //   PC 2: Halt             (not reached)
        //
        // a (PC 3-5):
        //   PC 3: Call(1, 0)       call "b"(0)
        //   PC 4: Halt             (not reached)
        //   PC 5: Halt             (not reached)
        //
        // b (PC 6-8):
        //   PC 6: Call(2, 0)       call "c"(0)
        //   PC 7: Halt             (not reached)
        //   PC 8: Halt             (not reached)
        //
        // c (PC 9-11):
        //   PC 9: PushInt(7)       push error value
        //   PC 10: Throw           throw 7 — no handler anywhere
        //   PC 11: Halt            (not reached)
        let program = make_multi_fn(
            vec![
                FastInstr::new(FastOp::Call, 0, 0),        // 0: main
                FastInstr::simple(FastOp::Halt),            // 1: (not reached)
                FastInstr::simple(FastOp::Halt),            // 2: (not reached)
                FastInstr::new(FastOp::Call, 1, 0),        // 3: a
                FastInstr::simple(FastOp::Halt),            // 4: (not reached)
                FastInstr::simple(FastOp::Halt),            // 5: (not reached)
                FastInstr::new(FastOp::Call, 2, 0),        // 6: b
                FastInstr::simple(FastOp::Halt),            // 7: (not reached)
                FastInstr::simple(FastOp::Halt),            // 8: (not reached)
                FastInstr::new(FastOp::PushInt, 7, 0),     // 9: c
                FastInstr::simple(FastOp::Throw),           // 10: throw — uncaught
                FastInstr::simple(FastOp::Halt),            // 11: (not reached)
            ],
            vec!["a", "b", "c"],
            vec![
                ("a", 3, 6, 0),
                ("b", 6, 9, 0),
                ("c", 9, 12, 0),
            ],
            0,
        );

        let mut vm = make_vm(program);
        let result = vm.run(10_000);

        assert!(result.is_err(),
            "Uncaught throw through 3 functions should return an error, got {:?}",
            result);
    }

    #[test]
    fn test_throw_unwind_three_functions_with_rethrow_from_handler() {
        // Verifies Throw unwinding through main → a → b → c where c throws,
        // a's handler catches and *re-throws* (another Throw), and main's
        // handler catches the re-throw. This exercises the two-phase unwind:
        // first Throw finds a's handler, second Throw walks past a's now-empty
        // handler stack to find main's handler.
        //
        // main (PCs 0-6):
        //   PC 0: EnterTry(6)      register handler at PC 6 (catches a's re-throw)
        //   PC 1: Call(0, 0)       call "a"(0)
        //   PC 2: Halt             (not reached — re-throw unwinds)
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
        //   PC 11: Throw           handler: re-throw (value 7 is on stack from first catch)
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
        //   PC 18: Throw           throw 7 → caught by a's handler, then re-thrown
        //   PC 19: Halt            (not reached)
        let program = make_multi_fn(
            vec![
                FastInstr::new(FastOp::EnterTry, 6, 0),   // 0: main
                FastInstr::new(FastOp::Call, 0, 0),       // 1: call "a"
                FastInstr::simple(FastOp::Halt),           // 2: (not reached)
                FastInstr::simple(FastOp::Halt),           // 3: (not reached)
                FastInstr::simple(FastOp::Halt),           // 4: (not reached)
                FastInstr::simple(FastOp::Halt),           // 5: (not reached)
                FastInstr::simple(FastOp::Halt),           // 6: handler — pops 7
                FastInstr::new(FastOp::EnterTry, 11, 0),  // 7: a
                FastInstr::new(FastOp::Call, 1, 0),       // 8: call "b"
                FastInstr::simple(FastOp::Halt),           // 9: (not reached)
                FastInstr::simple(FastOp::Halt),           // 10: (not reached)
                FastInstr::simple(FastOp::Throw),          // 11: handler: re-throw 7
                FastInstr::simple(FastOp::Halt),           // 12: (not reached)
                FastInstr::simple(FastOp::Halt),           // 13: (not reached)
                FastInstr::new(FastOp::Call, 2, 0),       // 14: b
                FastInstr::simple(FastOp::Halt),           // 15: (not reached)
                FastInstr::simple(FastOp::Halt),           // 16: (not reached)
                FastInstr::new(FastOp::PushInt, 7, 0),    // 17: c
                FastInstr::simple(FastOp::Throw),          // 18: throw 7
                FastInstr::simple(FastOp::Halt),           // 19: (not reached)
            ],
            vec!["a", "b", "c"],
            vec![
                ("a", 7, 14, 0),
                ("b", 14, 17, 0),
                ("c", 17, 20, 0),
            ],
            0,
        );

        let mut vm = make_vm(program);
        let result = vm.run(10_000);

        // c throws → a catches and re-throws → main catches. Result is Int(7).
        assert_eq!(result, FastYield::Finished(Some(RuntimeValue::Int(7))),
            "Rethrow through 3 functions: expected Int(7), got {:?}",
            result);
    }
}
