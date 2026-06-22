//! Portable CVM bytecode interpreter and VM trait abstraction.
//!
//! This module provides a portable VM implementation that can run
//! Crush bytecode without depending on nanovm's advanced features.

use crate::bytecode::{Program, instruction_size};
use crate::host::HostCaps;
use crate::vm::{Quotas, Value, VmError, VmResult};

/// A frame in the call stack.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Return instruction pointer (None for entry function).
    pub return_ip: Option<usize>,
    /// Local memory slots.
    pub memory: std::collections::HashMap<u16, Value>,
}

impl Frame {
    pub fn new(return_ip: Option<usize>) -> Self {
        Self {
            return_ip,
            memory: std::collections::HashMap::new(),
        }
    }
}

/// Portable bytecode interpreter.
///
/// This is a self-contained CVM1 bytecode interpreter that doesn't
/// depend on nanovm's advanced features (fastvm, arena, etc.).
/// It uses the same bytecode format and Value types as crush-vm.
mod opcodes;
pub struct PortableVm {
    /// The loaded program.
    program: Program,
    /// Execution quotas.
    quotas: Quotas,
    /// Host capabilities.
    host_caps: Option<HostCaps>,
    /// Declared capabilities (from manifest.permissions).
    declared_caps: std::collections::HashSet<String>,
    /// Function entry points (name -> instruction offset).
    func_entry: std::collections::HashMap<String, usize>,
    /// Call stack frames.
    call_stack: Vec<Frame>,
    /// Value stack.
    stack: Vec<Value>,
    /// Output parts.
    out_parts: Vec<String>,
    /// Current output length.
    out_len: usize,
    /// Instruction pointer.
    ip: usize,
    /// Total instruction steps executed.
    steps: usize,
    /// Whether the VM has halted.
    halted: bool,
    /// Whether privileged capabilities are allowed.
    privileged_allowed: bool,
    /// Exception handler stack (target IP for each active try block).
    try_stack: Vec<usize>,
    /// Next task ID for async spawn.
    next_task_id: u64,
    /// Scheduled tasks: task_id → function name.
    scheduled_tasks: std::collections::HashMap<u64, String>,
}

impl PortableVm {
    /// Create a new portable VM with the given program.
    pub fn new(program: Program) -> Self {
        // Extract permissions before moving program
        let permissions = program.manifest.permissions.clone();

        // Build function entry map
        let func_entry: std::collections::HashMap<String, usize> = program
            .manifest
            .functions
            .iter()
            .map(|(k, v)| (k.clone(), v.entry))
            .collect();

        // Determine entry point
        let start_ip = program
            .manifest
            .entry
            .as_deref()
            .and_then(|e| func_entry.get(e).copied())
            .unwrap_or_else(|| func_entry.values().copied().next().unwrap_or(0));

        // Initialize call stack with entry frame
        let mut call_stack = Vec::new();
        call_stack.push(Frame {
            return_ip: None,
            memory: std::collections::HashMap::new(),
        });

        let declared_caps: std::collections::HashSet<String> = permissions.into_iter().collect();

        Self {
            program,
            quotas: Quotas::default(),
            host_caps: None,
            declared_caps,
            func_entry,
            call_stack,
            stack: Vec::new(),
            out_parts: Vec::new(),
            out_len: 0,
            ip: start_ip,
            steps: 0,
            halted: false,
            privileged_allowed: false,
            try_stack: Vec::new(),
            next_task_id: 1,
            scheduled_tasks: std::collections::HashMap::new(),
        }
    }

    /// Set execution quotas.
    pub fn set_quotas(&mut self, quotas: Quotas) {
        self.quotas = quotas;
    }

    /// Register host capabilities.
    pub fn set_host_caps(&mut self, host_caps: HostCaps) {
        self.host_caps = Some(host_caps);
    }

    /// Allow or disallow privileged capabilities.
    pub fn set_privileged_allowed(&mut self, allowed: bool) {
        self.privileged_allowed = allowed;
    }

    /// Get the program.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Execute a single instruction.
    pub fn step(&mut self) -> Result<Option<VmYield>, VmError> {
        if self.halted {
            return Ok(None);
        }

        self.check_step_quota()?;
        self.check_stack_quota()?;

        let code = &self.program.code;
        let ip = self.ip;

        if ip >= code.len() {
            return Err(VmError::TruncatedInstruction(ip));
        }

        let opcode = code[ip];
        let next_ip = ip + instruction_size(opcode).ok_or(VmError::UnknownOpcode(opcode, ip))?;

        // Save IP before execution to detect control flow changes
        let ip_before = self.ip;
        opcodes::execute_instruction(self, opcode, next_ip)?;

        // Only advance IP if execute_instruction didn't change it
        // (CALL, RET, JMP, JZ, JNZ set self.ip themselves)
        if self.ip == ip_before {
            self.ip = next_ip;
        }
        self.steps += 1;

        Ok(None)
    }

    /// Run the VM until it halts or yields.
    pub fn run(&mut self) -> Result<VmResult, VmError> {
        loop {
            if let Some(_yield) = self.step()? {
                // Handle yield if needed
                return Ok(self.take_result());
            }
            if self.halted {
                return Ok(self.take_result());
            }
        }
    }

    /// Execute a single instruction at the current IP.

    fn get_function_entry(&self, name: &str) -> Result<usize, VmError> {
        self.func_entry
            .get(name)
            .copied()
            .ok_or_else(|| VmError::UnknownFunction(name.to_string()))
    }

    fn push(&mut self, v: Value) {
        self.stack.push(v);
    }

    fn pop(&mut self) -> Result<Value, VmError> {
        self.stack.pop().ok_or(VmError::StackUnderflow)
    }

    fn peek(&self) -> Result<&Value, VmError> {
        self.stack.last().ok_or(VmError::StackUnderflow)
    }

    fn check_step_quota(&self) -> Result<(), VmError> {
        if self.steps >= self.quotas.max_steps {
            return Err(VmError::StepQuota(self.quotas.max_steps));
        }
        Ok(())
    }

    fn check_stack_quota(&self) -> Result<(), VmError> {
        if self.stack.len() > self.quotas.max_stack {
            return Err(VmError::StackQuota(self.quotas.max_stack));
        }
        Ok(())
    }

    fn take_result(&self) -> VmResult {
        VmResult {
            output: self.out_parts.concat(),
            steps: self.steps,
            halted: self.halted,
            stack: self.stack.clone(),
        }
    }
}

/// Yield types for VM execution.
#[derive(Debug, Clone)]
pub enum VmYield {
    /// VM has finished execution.
    Finished,
    /// VM is waiting for a host capability.
    HostCall {
        capability: String,
        args: Vec<Value>,
    },
    /// VM has hit a breakpoint (for debugging).
    DebugBreak { reason: String },
}

/// Helper functions for array operations.
fn need_array(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Array(a) => Ok(a),
        other => Err(VmError::TypeError {
            expected: "array",
            got: value_type_name(&other),
        }),
    }
}

fn need_array_index(v: &Value) -> Result<i64, VmError> {
    match v {
        Value::Int(i) => Ok(*i),
        _ => Err(VmError::BadIndex(value_type_name(v))),
    }
}

fn wrap_index(idx: i64, len: usize) -> Result<usize, VmError> {
    if idx < 0 {
        let wrapped = (idx + len as i64) as usize;
        if wrapped >= len {
            return Err(VmError::ArrayBounds { index: idx, len });
        }
        Ok(wrapped)
    } else {
        let u = idx as usize;
        if u >= len {
            return Err(VmError::ArrayBounds { index: idx, len });
        }
        Ok(u)
    }
}

fn to_i64(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i64,
        _ => 0,
    }
}

fn to_f64_p(v: &Value) -> f64 {
    match v {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

/// Get a human-readable type name for a Value.
fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "str",
        Value::Array(_) => "array",
        Value::Map(_) => "map",
        Value::Error(_) => "error",
        Value::Bytes(_) => "bytes",
        Value::Handle(_) => "handle",
    }
}

/// Convert a Value to its text representation (for printing/string operations).
pub fn value_to_text(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Array(a) => {
            let items: Vec<String> = a.borrow().iter().map(value_to_text).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Map(m) => {
            let items: Vec<String> = m
                .borrow()
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_text(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        Value::Error(e) => format!("error({})", e),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        Value::Handle(id) => format!("<handle {}>", id),
    }
}

/// Check if a Value is truthy (Python-style truthiness).
fn value_is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.borrow().is_empty(),
        Value::Map(m) => !m.borrow().is_empty(),
        Value::Error(_) => true,
        Value::Bytes(b) => !b.is_empty(),
        Value::Handle(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::assemble;

    #[test]
    fn test_portable_vm_basic() {
        let source = r#"
            .func main
            PUSH_STR "hello"
            CAP_CALL "io.print" 1
            HALT
        "#;
        let program = assemble(source, Some(&["io.print"]), Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let result = vm.run().unwrap();
        assert_eq!(result.output, "hello");
        assert!(result.halted);
    }

    #[test]
    fn test_portable_vm_arithmetic() {
        let source = r#"
            .func main
            PUSH 10
            PUSH 5
            ADD
            CAP_CALL "io.print" 1
            HALT
        "#;
        let program = assemble(source, Some(&["io.print"]), Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let result = vm.run().unwrap();
        assert_eq!(result.output, "15");
    }

    #[test]
    fn test_portable_vm_function_call() {
        let source = r#"
            .func add
            ADD
            RET
            .func main
            PUSH 10
            PUSH 5
            CALL add
            CAP_CALL "io.print" 1
            HALT
        "#;
        let program = assemble(source, Some(&["io.print"]), Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let result = vm.run();
        match result {
            Ok(r) => {
                assert_eq!(r.output, "15");
            }
            Err(e) => {
                eprintln!("VM Error: {:?}", e);
                panic!("VM Error: {:?}", e);
            }
        }
    }

    #[test]
    fn test_portable_vm_array() {
        let source = r#"
            .func main
            PUSH 1
            PUSH 2
            PUSH 3
            NEW_ARRAY 3
            ARR_LEN
            CAP_CALL "io.print" 1
            HALT
        "#;
        let program = assemble(source, Some(&["io.print"]), Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let result = vm.run().unwrap();
        assert_eq!(result.output, "3");
    }
}
