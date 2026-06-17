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
        self.execute_instruction(opcode, next_ip)?;

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
    fn execute_instruction(&mut self, opcode: u8, next_ip: usize) -> Result<(), VmError> {
        use crate::bytecode::*;

        match opcode {
            NOP => {}
            PUSH => {
                let val = i64::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 9]
                        .try_into()
                        .unwrap(),
                );
                self.push(Value::Int(val));
            }
            PUSH_F64 => {
                let val = f64::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 9]
                        .try_into()
                        .unwrap(),
                );
                self.push(Value::Float(val));
            }
            PUSH_STR => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let s = self
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?;
                self.push(Value::Str(s.clone()));
            }
            PUSH_NULL => {
                self.push(Value::Null);
            }
            POP => {
                self.pop()?;
            }
            DUP => {
                let v = self.peek()?.clone();
                self.push(v);
            }
            SWAP => {
                let a = self.pop()?;
                let b = self.pop()?;
                self.push(a);
                self.push(b);
            }
            ADD | SUB | MUL | DIV | MOD => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match opcode {
                    ADD => Value::Int(to_i64(&a) + to_i64(&b)),
                    SUB => Value::Int(to_i64(&a) - to_i64(&b)),
                    MUL => Value::Int(to_i64(&a) * to_i64(&b)),
                    DIV => {
                        let denom = to_i64(&b);
                        if denom == 0 {
                            return Err(VmError::DivByZero);
                        }
                        Value::Int(to_i64(&a) / denom)
                    }
                    MOD => {
                        let denom = to_i64(&b);
                        if denom == 0 {
                            return Err(VmError::DivByZero);
                        }
                        Value::Int(to_i64(&a) % denom)
                    }
                    _ => unreachable!(),
                };
                self.push(result);
            }
            EQ | LT | GT => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match opcode {
                    EQ => Value::Bool(a == b),
                    LT => Value::Bool(to_i64(&a) < to_i64(&b)),
                    GT => Value::Bool(to_i64(&a) > to_i64(&b)),
                    _ => unreachable!(),
                };
                self.push(result);
            }
            NOT => {
                let a = self.pop()?;
                self.push(Value::Bool(!value_is_truthy(&a)));
            }
            LOAD => {
                let slot = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                );
                let frame = self.call_stack.last().ok_or(VmError::StackUnderflow)?;
                let v = frame.memory.get(&slot).ok_or(VmError::UninitSlot(slot))?;
                self.push(v.clone());
            }
            STORE => {
                let slot = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                );
                let v = self.pop()?;
                let frame = self.call_stack.last_mut().ok_or(VmError::StackUnderflow)?;
                frame.memory.insert(slot, v);
            }
            JMP | JZ | JNZ => {
                let target = u32::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 5]
                        .try_into()
                        .unwrap(),
                ) as usize;
                if target > self.program.code.len() {
                    return Err(VmError::BadJump(target));
                }
                let take = match opcode {
                    JMP => true,
                    JZ => !value_is_truthy(&self.pop()?),
                    JNZ => value_is_truthy(&self.pop()?),
                    _ => unreachable!(),
                };
                if take {
                    self.ip = target;
                }
            }
            PRINT => {
                let s = value_to_text(&self.pop()?);
                self.out_len += s.len();
                if self.out_len > self.quotas.max_output {
                    return Err(VmError::OutputQuota(self.quotas.max_output));
                }
                self.out_parts.push(s);
            }
            CAP_CALL => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let argc = self.program.code[self.ip + 3] as usize;

                let cap = self
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();

                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(self.pop()?);
                }
                args.reverse();

                let result = self.dispatch_cap(&cap, args)?;
                if let Some(v) = result {
                    self.push(v);
                }
            }
            CALL => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let fname = self
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?;

                let func_entry = self.get_function_entry(fname)?;
                if self.call_stack.len() >= self.quotas.max_call_depth {
                    return Err(VmError::CallDepthQuota(self.quotas.max_call_depth));
                }

                // For binary functions like add, pop the two arguments from main stack
                // and store them in the new frame's slots 0 and 1
                let mut frame = Frame::new(Some(next_ip));
                if self.stack.len() >= 2 {
                    // Pop args from main stack (top is last pushed)
                    let arg1 = self.pop()?; // 5
                    let arg0 = self.pop()?; // 10
                    frame.memory.insert(0, arg0); // slot 0 = first arg
                    frame.memory.insert(1, arg1); // slot 1 = second arg
                }
                self.call_stack.push(frame);
                self.ip = func_entry;
            }
            RET => {
                let return_value = self.pop()?;
                let frame = self.call_stack.pop().ok_or(VmError::StackUnderflow)?;
                match frame.return_ip {
                    None => {
                        self.push(return_value);
                        self.halted = true;
                    }
                    Some(ret_ip) => {
                        self.ip = ret_ip;
                        self.push(return_value);
                    }
                }
            }
            NEW_ARRAY => {
                let count = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(self.pop()?);
                }
                vals.reverse();
                self.push(Value::Array(vals));
            }
            ARR_GET => {
                let idx_v = self.pop()?;
                let arr_v = self.pop()?;
                let idx = need_array_index(&idx_v)?;
                let arr = need_array(&arr_v)?;
                let len = arr.len();
                let actual = wrap_index(idx, len)?;
                self.push(arr[actual].clone());
            }
            ARR_SET => {
                let val = self.pop()?;
                let idx_v = self.pop()?;
                let arr_v = self.pop()?;
                let idx = need_array_index(&idx_v)?;
                let mut arr = need_array(&arr_v)?.to_vec();
                let len = arr.len();
                let actual = wrap_index(idx, len)?;
                arr[actual] = val;
                self.push(Value::Array(arr));
            }
            ARR_LEN => {
                let v = self.pop()?;
                let len = need_array(&v)?.len();
                self.push(Value::Int(len as i64));
            }
            HALT => {
                self.halted = true;
            }
            _ => return Err(VmError::UnknownOpcode(opcode, self.ip)),
        }

        Ok(())
    }

    fn dispatch_cap(&mut self, cap: &str, args: Vec<Value>) -> Result<Option<Value>, VmError> {
        // Check permission
        if !self.declared_caps.contains(cap) {
            return Err(VmError::CapNotDeclared(cap.to_string()));
        }
        if let Some(allowed) = &self.quotas.allowed_caps
            && !allowed.iter().any(|a| a == cap)
        {
            return Err(VmError::CapDenied(cap.to_string()));
        }

        // Built-in portable capabilities
        if let Some(spec) = crate::caps::capabilities().get(cap) {
            if let Some(expected) = spec.argc
                && args.len() != expected
            {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected,
                    got: args.len(),
                });
            }
            return match cap {
                "io.print" => {
                    let s: String = args.iter().map(value_to_text).collect::<Vec<_>>().concat();
                    self.out_len += s.len();
                    if self.out_len > self.quotas.max_output {
                        return Err(VmError::OutputQuota(self.quotas.max_output));
                    }
                    self.out_parts.push(s);
                    Ok(None)
                }
                "str.concat" => {
                    let s: String = args.iter().map(value_to_text).collect::<Vec<_>>().concat();
                    Ok(Some(Value::Str(s)))
                }
                "str.len" => {
                    let s = value_to_text(&args[0]);
                    Ok(Some(Value::Int(s.len() as i64)))
                }
                _ => Err(VmError::UnknownCap(cap.to_string())),
            };
        }

        // Privilege check for host-provided capabilities
        if !self.privileged_allowed && crate::caps::is_privileged(cap) {
            return Err(VmError::CapDenied(format!(
                "privileged cap requires elevated grant: {cap}"
            )));
        }

        // Host-provided capabilities
        if let Some(host) = &self.host_caps
            && let Some(handler) = host.get(cap)
        {
            let spec = handler.spec();
            if let Some(expected) = spec.argc
                && args.len() != expected
            {
                return Err(VmError::CapArity {
                    cap: cap.to_string(),
                    expected,
                    got: args.len(),
                });
            }
            return handler
                .call(args)
                .map_err(|msg| VmError::UnknownCap(format!("{cap}: {msg}")));
        }

        Err(VmError::UnknownCap(cap.to_string()))
    }

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
fn need_array(v: &Value) -> Result<&Vec<Value>, VmError> {
    match v {
        Value::Array(a) => Ok(a),
        _ => Err(VmError::TypeError {
            expected: "array",
            got: value_type_name(v),
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
            let items: Vec<String> = a.iter().map(value_to_text).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Map(m) => {
            let items: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_text(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        Value::Error(e) => format!("error({})", e),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
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
        Value::Array(a) => !a.is_empty(),
        Value::Map(m) => !m.is_empty(),
        Value::Error(_) => true,
        Value::Bytes(b) => !b.is_empty(),
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
            LOAD 0
            LOAD 1
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
