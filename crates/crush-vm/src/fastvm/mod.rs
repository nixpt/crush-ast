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

pub use types::{FastYield, HostRequest, FastError, FastFrame};
pub use instructions::{FastInstr, FastOp, LoweredProgram, SymbolTables, HostCallSite, InterfaceCallSite, ExecLangSite, LowerError, lower_program};

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
            call_stack: Vec::with_capacity(32),
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
            
            // Push initial frame
            self.call_stack.push(FastFrame {
                return_pc: 0,
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
}
