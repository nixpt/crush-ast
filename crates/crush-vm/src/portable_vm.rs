//! | section | role |
//! |---------|------|
//! | `Frame` struct | One call-frame's worth of VM state (locals slot table + ip + jump-target cache). |
//! | `PortableVm` struct | The VM — owns the bytecode module + frames + value stack + declared-capability registry + quotas. |
//! | `PortableVm::step` | The dispatcher loop. Fetches one opcode, calls `execute_instruction`, applies pc-delta, handles `VmYield`. |
//! | `PortableVm::execute_instruction` | The large match-on-opcode (~660 lines) that interprets one instruction and mutates VM state. |
//! | `PortableVm::dispatch_cap` | The capability-dispatch chokepoint (~135 lines) — routes `Capability::Call` to a declared native op (io.print, str.*, math.*, make_range, ...). |
//! | Helpers | Smaller methods on `PortableVm` + the free fns `value_to_text` and `run`. |
//! | `VmYield` enum | The boxed-Future yield surface used by `PortableVm::run` to suspend on async capability calls. |
//! | `#[cfg(test)] mod tests` | A handful of inline tests living alongside the production code. |
//!
//! ## Existence as a single file (the why)
//!
//! When `portable_vm.rs` was first authored it was ~150 lines. Frame, the
//! PortableVm struct, `step`, `execute_instruction`, and `dispatch_cap`
//! were all written together as one cohesive module because they share
//! state through `&mut self` borrows and the boundary between
//! "VM operation" and "VM dispatch" is fuzzy at the function-call level
//! (e.g. `execute_instruction` calls `dispatch_cap` for capability
//! opcodes). The file has grown in place through CRUSHCN-1 (Frame
//! promotion, +Frame slot table), CRUSHFMT-1 (TOML-quoted byte literals),
//! CRUSHRUN-S2 (async + spawn), and several other arc landings.
//!
//! ## Future split intent (DEFERRED — see history below; do not pick up
//! without re-reading this comment and the linked analysis)
//!
//! The `CRUSHPVMSPLIT` ticket arc was triaged with intent to extract the
//! dispatch logic into `crates/crush-vm/src/portable_vm/opcodes.rs` — a
//! private submodule with `pub(super) fn execute_instruction` and
//! `pub(super) fn dispatch_cap` lifted out, leaving `PortableVm::step`
//! as the public dispatcher. After **seven** attempts (six on the full
//! combined extraction `CRUSHPVMSPLIT-1`, one on the smaller-scope
//! `CRUSHPVMSPLIT-1a` variant that extracted `dispatch_cap` only), the
//! brittle-transform risk turned out to exceed the maintenance burden of
//! leaving the dispatch inline. Both PRs (#11 and #12) are open and
//! unmerged at the time of this writing.
//!
//! If a future attempt wants to land CRUSHPVMSPLIT-1b (`execute_instruction`
//! move only, with `CRUSHPVMSPLIT-1a`'s `dispatch_cap` extraction as
//! prior art), the path that minimises regression is:
//!
//! 1. **Single atomic Python pass** — read `portable_vm.rs` once, walk
//!    braces depth-aware from the function signature to find the closing
//!    brace, apply all transforms in one shot
//!    (`self.foo`→`vm.foo`, `self.dispatch_cap(`→`dispatch_cap(vm, `,
//!    `Self::`→`super::PortableVm::`, bare `self`→`vm`), write both
//!    `opcodes.rs` AND the post-extraction `portable_vm.rs` from the
//!    same in-memory model. Multi-pass bash sed cascades were the cause
//!    of every prior failure.
//! 2. **Per-binary `^test crush_vm` diff vs `origin/main`** — must return
//!    *zero differences*, not just a raw test count match. The
//!    per-binary name diff catches test-discovery regressions that
//!    raw counts miss (off-by-one sed ranges can eat test fn names).
//! 3. **Land as `CRUSHPVMSPLIT-1b`, not a combined re-shot** — one
//!    function at a time; never combine the two extractions into one
//!    PR after the `1a` precedent.
//!
//! Until then, this file stays as one module.


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
    /// Exception handler stack (target IP for each active try block).
    try_stack: Vec<usize>,
    /// Next task ID for async spawn.
    next_task_id: u64,
    /// Scheduled tasks: task_id → (function name, args).
    scheduled_tasks: std::collections::HashMap<u64, (String, Vec<Value>)>,
    /// Bytecode-level breakpoints (instruction offsets).
    breakpoints: Vec<usize>,
    /// Per-IP counter: how many breakpoint hits have been reported
    /// for this IP without executing the instruction. When the count
    /// reaches the total number of breakpoints at that IP, the entry
    /// is cleared and the instruction executes.
    breakpoint_hit: std::collections::HashMap<usize, usize>,
    /// Per-IP total: how many breakpoints are registered at each
    /// bytecode offset. Precomputed in `set_breakpoints()` so the
    /// hot path in `step()` is an O(1) lookup instead of O(n)
    /// `.filter().count()`.
    breakpoint_count: std::collections::HashMap<usize, usize>,
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
            breakpoints: Vec::new(),
            breakpoint_hit: std::collections::HashMap::new(),
            breakpoint_count: std::collections::HashMap::new(),
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

    /// Whether the VM has halted (ret from entry frame).
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Register bytecode-level breakpoints. Each entry is an
    /// instruction offset in the program. When execution reaches
    /// one of these offsets, `step()` returns `DebugBreak` *before*
    /// executing the instruction.
    ///
    /// Resets per-IP hit counters so existing breakpoints that
    /// re-register at the same address fire again.
    pub fn set_breakpoints(&mut self, ips: &[usize]) {
        self.breakpoints = ips.to_vec();
        self.breakpoint_hit.clear();
        self.breakpoint_count.clear();
        for &ip in ips {
            *self.breakpoint_count.entry(ip).or_insert(0) += 1;
        }
    }

    /// Current instruction pointer (bytecode offset).
    pub fn current_ip(&self) -> usize {
        self.ip
    }

    /// Execute a single instruction.
    ///
    /// If a breakpoint is set at the current IP, returns
    /// `Ok(Some(VmYield::DebugBreak { .. }))` *before* executing
    /// the instruction. The instruction at the breakpoint IP is
    /// NOT executed — the VM is paused with IP unchanged.
    ///
    /// When multiple breakpoints are registered at the same IP,
    /// `step()` returns `DebugBreak` for each one in sequence
    /// before executing the instruction. A per-IP hit counter
    /// tracks how many have fired; the instruction executes only
    /// when all breakpoints at that IP have been reported.
    pub fn step(&mut self) -> Result<Option<VmYield>, VmError> {
        if self.halted {
            return Ok(None);
        }

        // Check breakpoints BEFORE execution. When multiple
        // breakpoints are registered at the same IP, each fires
        // separately before the instruction advances. The per-IP
        // counter tracks how many have already fired; the check
        // happens BEFORE incrementing so the Nth call fires the
        // Nth breakpoint rather than silently consuming it.
        if !self.breakpoints.is_empty() && self.breakpoint_count.contains_key(&self.ip) {
            let hit = self.breakpoint_hit.entry(self.ip).or_insert(0);
            let total = self.breakpoint_count.get(&self.ip).copied().unwrap_or(0);
            if *hit >= total {
                // All breakpoints at this IP have fired — clear and
                // fall through to execute.
                self.breakpoint_hit.remove(&self.ip);
            } else {
                *hit += 1;
                return Ok(Some(VmYield::DebugBreak {
                    reason: format!("breakpoint at ip {}", self.ip),
                }));
            }
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
            PUSH_BOOL => {
                let v = i64::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 9]
                        .try_into()
                        .unwrap(),
                );
                self.push(Value::Bool(v != 0));
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
            ROT => {
                let a = self.pop()?;
                let b = self.pop()?;
                let c = self.pop()?;
                self.push(b);
                self.push(c);
                self.push(a);
            }
            PICK => {
                let n = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap(),
                ) as usize;
                if n >= self.stack.len() {
                    return Err(VmError::StackUnderflow);
                }
                self.push(self.stack[self.stack.len() - 1 - n].clone());
            }
            ROLL => {
                let n = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap(),
                ) as usize;
                if n >= self.stack.len() {
                    return Err(VmError::StackUnderflow);
                }
                let idx = self.stack.len() - 1 - n;
                let v = self.stack.remove(idx);
                self.push(v);
            }
            // `+` concatenates when EITHER side is a string. This MUST match scheduler.rs exactly:
            // the two VMs run the same programs, and a divergence here is a silent miscompile.
            //
            // It was already diverging. This arm did not guard its operands, and to_f64_p ends in
            // `_ => 0.0` — so portable_vm silently evaluated `"a" + "b"` to 0 and `"x: " + 5` to 5,
            // while scheduler.rs raised a TypeError on the very same source. No error either way.
            ADD if matches!(self.peek_n(0), Some(Value::Str(_)))
                || matches!(self.peek_n(1), Some(Value::Str(_))) =>
            {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(Value::Str(format!("{}{}", a.as_text(), b.as_text())));
            }
            // Anything else non-numeric is a LOUD error, not a silent 0.
            ADD | SUB | MUL | DIV | MOD
                if !matches!(self.peek_n(0), Some(Value::Int(_)) | Some(Value::Float(_)))
                    || !matches!(self.peek_n(1), Some(Value::Int(_)) | Some(Value::Float(_))) =>
            {
                let got = self.peek_n(0).map(value_type_name).unwrap_or("nothing");
                return Err(VmError::TypeError { expected: "numeric", got });
            }
            ADD | SUB | MUL | DIV | MOD => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match opcode {
                    ADD => crate::arithmetic::add_values(&a, &b)?,
                    SUB => crate::arithmetic::sub_values(&a, &b)?,
                    MUL => crate::arithmetic::mul_values(&a, &b)?,
                    DIV => crate::arithmetic::div_values(&a, &b)?,
                    MOD => crate::arithmetic::mod_values(&a, &b)?,
                    _ => unreachable!(),
                };
                self.push(result);
            }
            NEG => {
                let a = self.pop()?;
                self.push(crate::arithmetic::neg_value(&a)?);
            }
            MATH_POW => {
                let exp = self.pop()?;
                let base = self.pop()?;
                let base_f = to_f64_p(&base);
                let exp_f = to_f64_p(&exp);
                self.push(Value::Float(base_f.powf(exp_f)));
            }
            MATH_SQRT | MATH_ABS | MATH_ROUND | MATH_FLOOR | MATH_CEIL => {
                let a = self.pop()?;
                let af = to_f64_p(&a);
                let res = match opcode {
                    MATH_SQRT => af.sqrt(),
                    MATH_ABS => af.abs(),
                    MATH_ROUND => af.round(),
                    MATH_FLOOR => af.floor(),
                    MATH_CEIL => af.ceil(),
                    _ => unreachable!(),
                };
                self.push(Value::Float(res));
            }
            VEC_ADD => {
                let right = self.pop()?;
                let left = self.pop()?;
                if let (Value::Array(a), Value::Array(b)) = (&left, &right) {
                    let a_ref = a.borrow();
                    let b_ref = b.borrow();
                    let len = a_ref.len().min(b_ref.len());
                    let mut res = Vec::with_capacity(len);
                    for i in 0..len {
                        let va = to_f64_p(&a_ref[i]);
                        let vb = to_f64_p(&b_ref[i]);
                        res.push(Value::Float(va + vb));
                    }
                    self.push(Value::new_array(res));
                } else {
                    return Err(VmError::TypeError {
                        expected: "array",
                        got: "non-array in VEC_ADD".into(),
                    });
                }
            }
            VEC_DOT => {
                let right = self.pop()?;
                let left = self.pop()?;
                if let (Value::Array(a), Value::Array(b)) = (&left, &right) {
                    let a_ref = a.borrow();
                    let b_ref = b.borrow();
                    let len = a_ref.len().min(b_ref.len());
                    let mut sum = 0.0;
                    for i in 0..len {
                        let va = to_f64_p(&a_ref[i]);
                        let vb = to_f64_p(&b_ref[i]);
                        sum += va * vb;
                    }
                    self.push(Value::Float(sum));
                } else {
                    return Err(VmError::TypeError {
                        expected: "array",
                        got: "non-array in VEC_DOT".into(),
                    });
                }
            }
            MAT_MUL => {
                // Naive Matrix Multiply assuming arrays of arrays (List[List[float]])
                let right = self.pop()?;
                let left = self.pop()?;
                if let (Value::Array(a), Value::Array(b)) = (&left, &right) {
                    let a_ref = a.borrow();
                    let b_ref = b.borrow();
                    let mut res = Vec::new();
                    // Just fallback to returning null if shape is bad for now
                    if a_ref.is_empty() || b_ref.is_empty() {
                        self.push(Value::new_array(vec![]));
                    } else {
                        let rows_a = a_ref.len();
                        let cols_a = if let Value::Array(first) = &a_ref[0] { first.borrow().len() } else { 0 };
                        let cols_b = if let Value::Array(first) = &b_ref[0] { first.borrow().len() } else { 0 };
                        
                        for i in 0..rows_a {
                            let mut row_res = Vec::new();
                            for j in 0..cols_b {
                                let mut sum = 0.0;
                                for k in 0..cols_a {
                                    let val_a = if let Value::Array(r) = &a_ref[i] { to_f64_p(&r.borrow()[k]) } else { 0.0 };
                                    let val_b = if let Value::Array(r) = &b_ref[k] { to_f64_p(&r.borrow()[j]) } else { 0.0 };
                                    sum += val_a * val_b;
                                }
                                row_res.push(Value::Float(sum));
                            }
                            res.push(Value::new_array(row_res));
                        }
                        self.push(Value::new_array(res));
                    }
                } else {
                    return Err(VmError::TypeError {
                        expected: "array",
                        got: "non-array in MAT_MUL".into(),
                    });
                }
            }
            EQ | NE => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(match opcode {
                    EQ => Value::Bool(a == b),
                    NE => Value::Bool(a != b),
                    _ => unreachable!(),
                });
            }
            LT | GT | LE | GE => {
                let b = self.pop()?;
                let a = self.pop()?;
                let result = match opcode {
                    LT => crate::arithmetic::compare_values(&a, &b, |x, y| x < y)?,
                    GT => crate::arithmetic::compare_values(&a, &b, |x, y| x > y)?,
                    LE => crate::arithmetic::compare_values(&a, &b, |x, y| x <= y)?,
                    GE => crate::arithmetic::compare_values(&a, &b, |x, y| x >= y)?,
                    _ => unreachable!(),
                };
                self.push(result);
            }
            AND | OR => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(match opcode {
                    AND => Value::Bool(value_is_truthy(&a) && value_is_truthy(&b)),
                    OR => Value::Bool(value_is_truthy(&a) || value_is_truthy(&b)),
                    _ => unreachable!(),
                });
            }
            BITAND | BITOR | BITXOR | SHL | SHR => {
                let b = self.pop()?;
                let a = self.pop()?;
                let ai = to_i64(&a);
                let bi = to_i64(&b);
                let result = match opcode {
                    BITAND => Value::Int(ai & bi),
                    BITOR => Value::Int(ai | bi),
                    BITXOR => Value::Int(ai ^ bi),
                    SHL => Value::Int(
                        ai.checked_shl(bi as u32)
                            .ok_or(VmError::ArithmeticOverflow)?,
                    ),
                    SHR => Value::Int(
                        ai.checked_shr(bi as u32)
                            .ok_or(VmError::ArithmeticOverflow)?,
                    ),
                    _ => unreachable!(),
                };
                self.push(result);
            }
            BITNOT => {
                let a = self.pop()?;
                self.push(Value::Int(!to_i64(&a)));
            }
            NOT => {
                let a = self.pop()?;
                self.push(Value::Bool(!value_is_truthy(&a)));
            }
            TYPEOF => {
                let v = self.pop()?;
                self.push(Value::Str(value_type_name(&v).to_string()));
            }
            CAST => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap(),
                ) as usize;
                let type_name = self.program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
                let v = self.pop()?;
                match type_name.as_str() {
                    "str" | "string" => self.push(Value::Str(value_to_text(&v))),
                    "int" | "i64" => {
                        self.push(match v {
                            Value::Int(_) => v,
                            Value::Float(f) => Value::Int(f as i64),
                            Value::Str(s) => Value::Int(s.parse().unwrap_or(0)),
                            Value::Bool(b) => Value::Int(if b { 1 } else { 0 }),
                            _ => Value::Int(0),
                        });
                    }
                    "float" | "f64" => {
                        self.push(match v {
                            Value::Float(_) => v,
                            Value::Int(i) => Value::Float(i as f64),
                            Value::Str(s) => Value::Float(s.parse().unwrap_or(0.0)),
                            Value::Bool(b) => Value::Float(if b { 1.0 } else { 0.0 }),
                            _ => Value::Float(0.0),
                        });
                    }
                    "bool" => self.push(Value::Bool(value_is_truthy(&v))),
                    _ => self.push(v),
                }
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

                // Arguments stay on the stack (main VM convention).
                // Callee accesses them via stack operations or LOAD/STORE slots.
                self.call_stack.push(Frame::new(Some(next_ip)));
                self.ip = func_entry;
            }
            RET => {
                let frame = self.call_stack.pop().ok_or(VmError::StackUnderflow)?;
                match frame.return_ip {
                    None => {
                        self.halted = true;
                    }
                    Some(ret_ip) => {
                        self.ip = ret_ip;
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
                self.push(Value::new_array(vals));
            }
            ARR_GET => {
                let idx_v = self.pop()?;
                let arr_v = self.pop()?;
                let idx = need_array_index(&idx_v)?;
                match arr_v {
                    Value::Array(arr_rc) => {
                        let arr = arr_rc.borrow();
                        let len = arr.len();
                        let actual = wrap_index(idx, len)?;
                        self.push(arr[actual].clone());
                    }
                    Value::Str(s) => {
                        let len = s.chars().count();
                        let actual = wrap_index(idx, len)?;
                        let ch = s.chars().nth(actual).map(|c| c.to_string()).unwrap_or_default();
                        self.push(Value::Str(ch));
                    }
                    _ => return Err(VmError::TypeError { expected: "array or string", got: arr_v.type_name() }),
                }
            }
            ARR_SET => {
                let val = self.pop()?;
                let idx_v = self.pop()?;
                let arr_v = self.pop()?;
                let idx = need_array_index(&idx_v)?;
                let arr_rc = need_array(arr_v)?;
                {
                    let mut arr = arr_rc.borrow_mut();
                    let len = arr.len();
                    let actual = wrap_index(idx, len)?;
                    arr[actual] = val;
                }
                self.push(Value::Array(arr_rc));
            }
            ARR_LEN => {
                let v = self.pop()?;
                let arr_rc = need_array(v)?;
                self.push(Value::Int(arr_rc.borrow().len() as i64));
            }
            ARR_PUSH => {
                let val = self.pop()?;
                let arr_rc = need_array(self.pop()?)?;
                arr_rc.borrow_mut().push(val);
                self.push(Value::Array(arr_rc));
            }
            ARR_POP => {
                let arr_rc = need_array(self.pop()?)?;
                let val = arr_rc.borrow_mut().pop().unwrap_or(Value::Null);
                self.push(Value::Array(arr_rc.clone()));
                self.push(val);
            }
            NEW_TUPLE => {
                let count = u16::from_be_bytes(self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap()) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(self.pop()?);
                }
                vals.reverse();
                self.push(Value::new_tuple(vals));
            }
            TUPLE_PUSH => {
                let val = self.pop()?;
                let mut t = need_tuple(self.pop()?)?;
                t.push(val);
                self.push(Value::Tuple(t));
            }
            NEW_LIST => {
                let count = u16::from_be_bytes(self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap()) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(self.pop()?);
                }
                vals.reverse();
                self.push(Value::new_list(vals));
            }
            LIST_PUSH => {
                let val = self.pop()?;
                let l_rc = need_list(self.pop()?)?;
                l_rc.borrow_mut().push_back(val);
                self.push(Value::List(l_rc));
            }
            NEW_VECTOR => {
                let count = u16::from_be_bytes(self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap()) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(self.pop()?);
                }
                vals.reverse();
                self.push(Value::new_vector(vals));
            }
            VECTOR_PUSH => {
                let val = self.pop()?;
                let v_rc = need_vector(self.pop()?)?;
                v_rc.borrow_mut().push(val);
                self.push(Value::Vector(v_rc));
            }
            NEW_SET => {
                let count = u16::from_be_bytes(self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap()) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(self.pop()?);
                }
                vals.reverse();
                self.push(Value::new_set(vals));
            }
            SET_PUSH => {
                let val = self.pop()?;
                let s_rc = need_set(self.pop()?)?;
                s_rc.borrow_mut().push(val); // In actual Set might check uniqueness, using Vec for now
                self.push(Value::Set(s_rc));
            }
            NEW_OBJ => {
                self.push(Value::new_map(std::collections::HashMap::new()));
            }
            SET_FIELD => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let field = self
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let val = self.pop()?;
                let map_rc = match self.pop()? {
                    Value::Map(m) => m,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "map",
                            got: value_type_name(&other),
                        });
                    }
                };
                map_rc.borrow_mut().insert(field, val);
                self.push(Value::Map(map_rc));
            }
            GET_FIELD => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3]
                        .try_into()
                        .unwrap(),
                ) as usize;
                let field = self
                    .program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let map_rc = match self.pop()? {
                    Value::Map(m) => m,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "map",
                            got: value_type_name(&other),
                        });
                    }
                };
                let val = map_rc.borrow().get(&field).cloned().unwrap_or(Value::Null);
                self.push(val);
            }
            ENTER_TRY => {
                let target = u32::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 5]
                        .try_into()
                        .unwrap(),
                ) as usize;
                if target > self.program.code.len() {
                    return Err(VmError::BadJump(target));
                }
                self.try_stack.push(target);
            }
            EXIT_TRY => {
                self.try_stack.pop();
            }
            THROW => {
                let err_val = self.pop()?;
                if let Some(handler_ip) = self.try_stack.pop() {
                    self.ip = handler_ip;
                    self.push(err_val);
                    return Ok(());
                }
                return Err(VmError::UnknownCap(format!(
                    "uncaught error: {}",
                    value_to_text(&err_val)
                )));
            }
            STR_CONTAINS | STR_STARTS_WITH | STR_ENDS_WITH => {
                let needle = self.pop()?;
                let haystack = self.pop()?;
                let haystack_str = value_to_text(&haystack);
                let pattern_str = value_to_text(&needle);
                let res = match opcode {
                    STR_CONTAINS => haystack_str.contains(&pattern_str),
                    STR_STARTS_WITH => haystack_str.starts_with(&pattern_str),
                    STR_ENDS_WITH => haystack_str.ends_with(&pattern_str),
                    _ => unreachable!(),
                };
                self.push(Value::Bool(res));
            }
            STR_TO_UPPER | STR_TO_LOWER | STR_TRIM => {
                let s = self.pop()?;
                let text = value_to_text(&s);
                let res = match opcode {
                    STR_TO_UPPER => text.to_uppercase(),
                    STR_TO_LOWER => text.to_lowercase(),
                    STR_TRIM => text.trim().to_string(),
                    _ => unreachable!(),
                };
                self.push(Value::Str(res));
            }
            STR_SPLIT => {
                let delim = self.pop()?;
                let s = self.pop()?;
                let text = value_to_text(&s);
                let d = value_to_text(&delim);
                let parts: Vec<Value> = if d.is_empty() {
                    text.chars().map(|c| Value::Str(c.to_string())).collect()
                } else {
                    text.split(&d).map(|p| Value::Str(p.to_string())).collect()
                };
                self.push(Value::new_array(parts));
            }
            STR_REPLACE => {
                let to = self.pop()?;
                let from = self.pop()?;
                let s = self.pop()?;
                self.push(Value::Str(
                    value_to_text(&s).replace(&value_to_text(&from), &value_to_text(&to)),
                ));
            }
            STR_JOIN => {
                let delim = self.pop()?;
                let arr_v = self.pop()?;
                let d = value_to_text(&delim);
                match arr_v {
                    Value::Array(elems) => {
                        let parts: Vec<String> = elems.borrow().iter().map(|v| value_to_text(v)).collect();
                        self.push(Value::Str(parts.join(&d)));
                    }
                    other => {
                        return Err(VmError::TypeError {
                            expected: "array",
                            got: value_type_name(&other),
                        });
                    }
                }
            }
            MAKE_RANGE => {
                let end_v = self.pop()?;
                let start_v = self.pop()?;
                let start = match start_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: value_type_name(&other),
                        });
                    }
                };
                let end = match end_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: value_type_name(&other),
                        });
                    }
                };
                let mut elems = Vec::new();
                if start < end {
                    for i in start..end {
                        elems.push(Value::Int(i));
                    }
                }
                self.push(Value::new_array(elems));
            }
            EXEC_LANG => {
                let idx = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap(),
                ) as usize;
                let spec_json = self.program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
                let spec: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_str(&spec_json)
                        .map_err(|_| VmError::UnknownCap("exec_lang: invalid args JSON".to_string()))?;
                let lang = spec.get("lang").and_then(|v| v.as_str()).unwrap_or("?");
                let code_str = spec.get("code").and_then(|v| v.as_str()).unwrap_or("");
                let var_count = spec.get("var_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let mut var_names: Vec<String> = Vec::with_capacity(var_count);
                let mut var_values: Vec<Value> = Vec::with_capacity(var_count);
                for i in 0..var_count {
                    let key = format!("var_{}", i);
                    if let Some(name) = spec.get(&key).and_then(|v| v.as_str()) {
                        var_names.push(name.to_string());
                        var_values.push(self.pop()?);
                    }
                }
                var_values.reverse();
                // Use the SAME allowlist as scheduler.rs — not Command::new(lang).arg("-c").
                // crush-diff caught this drifting: `javascript` needs `node -e`, not a binary
                // named "javascript" with `-c`. An unknown language is a loud error, never a
                // silent spawn attempt.
                let (binary, exec_flag) = crate::scheduler::resolve_lang_binary(lang)
                    .ok_or_else(|| VmError::UnknownCap(format!("no executor registered for language '{lang}'")))?;
                // CAPABILITY GATE — must match scheduler.rs exactly (crush-diff would catch drift).
                // A @lang block spawns an interpreter with full host authority; require polyglot.<lang>.
                let gate = crate::scheduler::canonical_lang(lang)
                    .map(|c| format!("polyglot.{c}"))
                    .unwrap_or_else(|| format!("polyglot.{lang}"));
                if self.host_caps.as_ref().map(|h| h.get(&gate).is_none()).unwrap_or(true) {
                    return Err(VmError::UnknownCap(format!(
                        "@{lang} requires the '{gate}' capability (run with --polyglot to grant it); refusing to spawn"
                    )));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = (binary, exec_flag, code_str, &var_names, &var_values);
                    return Err(VmError::UnknownCap(format!(
                        "@{lang}: polyglot subprocess execution is not supported on wasm32 targets"
                    )));
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let mut cmd = std::process::Command::new(binary);
                    cmd.arg(exec_flag).arg(code_str);
                    for (name, val) in var_names.iter().zip(var_values.iter()) {
                        cmd.env(name, value_to_text(val));
                    }
                    // Same wall-clock bound as scheduler.rs — see
                    // `run_with_wall_clock_limit`'s doc comment for why this
                    // covers EXEC_LANG specifically and not CAP_CALL generically.
                    let output = match crate::scheduler::run_with_wall_clock_limit(
                        cmd,
                        self.quotas.max_wall_time_ms,
                    )
                    .map_err(|e| VmError::UnknownCap(format!("exec_lang({lang}): {e}")))?
                    {
                        crate::scheduler::CommandOutcome::Output(output) => output,
                        crate::scheduler::CommandOutcome::TimedOut => {
                            return Err(VmError::CapTimeout {
                                cap: gate,
                                limit_ms: self.quotas.max_wall_time_ms,
                            });
                        }
                    };
                    if output.status.success() {
                        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        self.out_len += s.len();
                        if self.out_len > self.quotas.max_output {
                            return Err(VmError::OutputQuota(self.quotas.max_output));
                        }
                        self.out_parts.push(s.clone());
                        self.push(Value::Str(s));
                    } else {
                        let err = String::from_utf8_lossy(&output.stderr);
                        return Err(VmError::UnknownCap(format!("exec_lang({lang}): {err}")));
                    }
                }
            }
            AI_QUERY | AI_SYNTHESIZE | AI_AGENT_DELEGATION | AI_SEMANTIC_MATCH | AI_LEARNING_LOOP | AI_CONTEXT_AWARE | AI_TOOLCHAIN => {
                // Portable VM does not support async AI opcodes. Stub to Null.
                self.push(Value::Null);
            }
            SPAWN => {
                let argc = u16::from_be_bytes(
                    self.program.code[self.ip + 1..self.ip + 3].try_into().unwrap(),
                ) as usize;
                let fn_name = value_to_text(&self.pop()?);
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(self.pop()?);
                }
                args.reverse();
                let task_id = self.next_task_id;
                self.next_task_id += 1;
                self.scheduled_tasks.insert(task_id, (fn_name, args));
                self.push(Value::Int(task_id as i64));
            }
            YIELD => {
                // No OS threads on wasm32 — cooperative single-threaded execution
                // already yields nothing to yield to, so this is a no-op there.
                #[cfg(not(target_arch = "wasm32"))]
                std::thread::yield_now();
            }
            AWAIT => {
                let handle = self.pop()?;
                if let Value::Int(task_id) = handle {
                    if let Some((fn_name, args)) = self.scheduled_tasks.remove(&(task_id as u64)) {
                        if let Some(&entry) = self.func_entry.get(&fn_name) {
                            if self.call_stack.len() >= self.quotas.max_call_depth {
                                return Err(VmError::CallDepthQuota(self.quotas.max_call_depth));
                            }
                            self.stack.extend(args);
                            self.call_stack
                                .push(Frame::new(Some(next_ip)));
                            self.ip = entry;
                            return Ok(());
                        }
                    }
                }
                self.push(Value::Null);
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
                    let parts: Vec<String> = args.iter().map(value_to_text).collect();
                    let line = crate::io_print::format_io_print_line(&parts);
                    self.out_len += line.len();
                    if self.out_len > self.quotas.max_output {
                        return Err(VmError::OutputQuota(self.quotas.max_output));
                    }
                    self.out_parts.push(line);
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
                "str.contains" => {
                    let haystack = value_to_text(&args[0]);
                    let needle = value_to_text(&args[1]);
                    Ok(Some(Value::Bool(haystack.contains(&needle))))
                }
                "str.split" => {
                    let s = value_to_text(&args[0]);
                    let delim = value_to_text(&args[1]);
                    let parts: Vec<Value> = if delim.is_empty() {
                        s.chars().map(|c| Value::Str(c.to_string())).collect()
                    } else {
                        s.split(&delim).map(|p| Value::Str(p.to_string())).collect()
                    };
                    Ok(Some(Value::new_array(parts)))
                }
                "str.replace" => {
                    let s = value_to_text(&args[0]);
                    let from = value_to_text(&args[1]);
                    let to = value_to_text(&args[2]);
                    Ok(Some(Value::Str(s.replace(&from, &to))))
                }
                "str.join" => {
                    let delim = value_to_text(&args[1]);
                    match &args[0] {
                        Value::Array(elems) => {
                            let parts: Vec<String> =
                                elems.borrow().iter().map(|v| value_to_text(v)).collect();
                            Ok(Some(Value::Str(parts.join(&delim))))
                        }
                        other => Err(VmError::TypeError {
                            expected: "array",
                            got: value_type_name(other),
                        }),
                    }
                }
                                "make_range" => {
                    let (start, end) = match args.len() {
                        0 => (0i64, 100i64),
                        1 => {
                            let end = match &args[0] { Value::Int(i) => *i, _ => 100 };
                            (0, end.max(0))
                        }
                        _ => {
                            let s = match &args[0] { Value::Int(i) => *i, _ => 0 };
                            let e = match &args[1] { Value::Int(i) => *i, _ => 0 };
                            (s, e)
                        }
                    };
                    let mut elems = Vec::new();
                    if start < end { for i in start..end { elems.push(Value::Int(i)); } }
                    Ok(Some(Value::new_array(elems)))
                }
                "append" | "push" => {
                    if args.len() < 2 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 2, got: args.len() }); }
                    match &args[0] {
                        Value::Array(elems) => { elems.borrow_mut().push(args[1].clone()); Ok(Some(args[0].clone())) }
                        _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                    }
                }
                "arr_set" => {
                    if args.len() < 3 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 3, got: args.len() }); }
                    match &args[0] {
                        Value::Array(elems) => {
                            let idx = match &args[1] { Value::Int(i) => *i as usize, _ => 0 };
                            let mut arr = elems.borrow_mut();
                            if idx < arr.len() { arr[idx] = args[2].clone(); }
                            Ok(Some(args[0].clone()))
                        }
                        _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                    }
                }
                "arr_get" => {
                    if args.len() < 2 { return Err(VmError::CapArity { cap: cap.to_string(), expected: 2, got: args.len() }); }
                    match &args[0] {
                        Value::Array(elems) => {
                            let idx = match &args[1] { Value::Int(i) => *i as usize, _ => 0 };
                            Ok(Some(elems.borrow().get(idx).cloned().unwrap_or(Value::Null)))
                        }
                        _ => Err(VmError::TypeError { expected: "array", got: args[0].type_name() }),
                    }
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

    /// Peek `n` values down the stack without popping (0 = top).
    ///
    /// The ADD arms must inspect operand TYPES before committing to a numeric or a string
    /// interpretation, which they cannot do after popping.
    fn peek_n(&self, n: usize) -> Option<&Value> {
        self.stack.len().checked_sub(n + 1).and_then(|k| self.stack.get(k))
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
        other => Err(VmError::TypeError { expected: "array", got: value_type_name(&other) }),
    }
}

fn need_tuple(v: Value) -> Result<Vec<Value>, VmError> {
    match v {
        Value::Tuple(t) => Ok(t),
        other => Err(VmError::TypeError { expected: "tuple", got: value_type_name(&other) }),
    }
}

fn need_list(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<std::collections::LinkedList<Value>>>, VmError> {
    match v {
        Value::List(l) => Ok(l),
        other => Err(VmError::TypeError { expected: "list", got: value_type_name(&other) }),
    }
}

fn need_vector(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Vector(v_rc) => Ok(v_rc),
        other => Err(VmError::TypeError { expected: "vector", got: value_type_name(&other) }),
    }
}

fn need_set(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Set(s) => Ok(s),
        other => Err(VmError::TypeError { expected: "set", got: value_type_name(&other) }),
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
        Value::Tuple(_) => "tuple",
        Value::List(_) => "list",
        Value::Vector(_) => "vector",
        Value::Set(_) => "set",
        Value::Map(_) => "map",
        Value::Error(_) => "error",
        Value::Bytes(_) => "bytes",
        Value::Handle(_) => "handle",
        Value::Foreign(_) => "foreign",
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
        Value::Tuple(t) => {
            let items: Vec<String> = t.iter().map(value_to_text).collect();
            format!("({})", items.join(", "))
        }
        Value::List(l) => {
            let items: Vec<String> = l.borrow().iter().map(value_to_text).collect();
            format!("List[{}]", items.join(", "))
        }
        Value::Vector(v) => {
            let items: Vec<String> = v.borrow().iter().map(value_to_text).collect();
            format!("Vector[{}]", items.join(", "))
        }
        Value::Set(s) => {
            let items: Vec<String> = s.borrow().iter().map(value_to_text).collect();
            format!("Set{{{}}}", items.join(", "))
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
        Value::Foreign(id) => format!("<foreign {}>", id),
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
        Value::Tuple(t) => !t.is_empty(),
        Value::List(l) => !l.borrow().is_empty(),
        Value::Vector(v) => !v.borrow().is_empty(),
        Value::Set(s) => !s.borrow().is_empty(),
        Value::Map(m) => !m.borrow().is_empty(),
        Value::Error(_) => true,
        Value::Bytes(b) => !b.is_empty(),
        Value::Handle(_) => true,
        Value::Foreign(_) => true,
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
        assert_eq!(result.output, "hello\n");
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
        assert_eq!(result.output, "15\n");
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
                assert_eq!(r.output, "15\n");
            }
            Err(e) => {
                eprintln!("VM Error: {:?}", e);
                panic!("VM Error: {:?}", e);
            }
        }
    }

    #[test]
    fn test_portable_vm_spawn_with_args() {
        let source = r#"
            .func main
            PUSH 99
            PUSH_STR "double"
            SPAWN 1
            AWAIT
            HALT
            .func double
            PUSH 2
            MUL
            RET
        "#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let result = vm.run().unwrap();
        assert_eq!(result.stack, vec![Value::Int(198)]);
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
        assert_eq!(result.output, "3\n");
    }

    // ── parity tests (mirror canonical src/tests.rs for these opcodes) ─────
    //
    // The canonical VM in `src/tests.rs` exercises each operand/type via
    // `run_src(<source>) -> VmResult`. These parity tests do the same shape
    // against `PortableVm` to lock the implementations in lockstep. Naming
    // uses a `test_portable_` prefix so they cannot collide with the canonical
    // `mod tests` once both run in the same `cargo test --workspace` build.
    //
    // EXEC_LANG parity is verified by `test_portable_exec_lang_partial_binding`
    // below — the pop-on-name pattern used here matches the canonical
    // green-thread scheduler (scheduler.rs EXEC_LANG arm). Both VMs now
    // produce identical subprocess env-var sets on any EXEC_LANG-shaped
    // program, including the partial-binding case (var_count > name_count).

    #[test]
    fn test_portable_push_bool() {
        // PUSH_BOOL operand is i64 (0/1); map to bool value via `v != 0`.
        let program = assemble("PUSH_BOOL 1\nHALT", None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack, vec![Value::Bool(true)]);

        let program = assemble("PUSH_BOOL 0\nHALT", None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack, vec![Value::Bool(false)]);
    }

    #[test]
    fn test_portable_new_obj_creates_empty_map() {
        let program = assemble("NEW_OBJ\nHALT", None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack.len(), 1);
        assert!(matches!(r.stack[0], Value::Map(_)));
    }

    #[test]
    fn test_portable_set_field_and_get_field() {
        let source = r#"NEW_OBJ
DUP
PUSH_STR "hello"
SET_FIELD "greeting"
GET_FIELD "greeting"
HALT"#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack.len(), 2);
        assert!(matches!(r.stack[0], Value::Map(_)));
        assert_eq!(r.stack[1], Value::Str("hello".to_string()));
    }

    #[test]
    fn test_portable_get_field_missing_returns_null() {
        let program = assemble(
            r#"NEW_OBJ
GET_FIELD "missing"
HALT"#,
            None,
            Some("test"),
        )
        .unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack, vec![Value::Null]);
    }

    #[test]
    fn test_portable_map_type_name() {
        // Mirrors canonical `tests.rs::map_type_name` (the `Value::type_name`
        // method is `pub(crate)` on `crush_vm::vm::Value`, so the test — in the
        // same crate — can call it directly).
        let program = assemble("NEW_OBJ\nHALT", None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert!(matches!(r.stack[0], Value::Map(_)));
        assert_eq!(r.stack[0].type_name(), "map");
    }

    #[test]
    fn test_portable_throw_basic() {
        // Uncaught THROW (no enter_try on the stack) must produce an error.
        let program = assemble(
            r#"PUSH_STR "oops"
THROW
HALT"#,
            None,
            Some("test"),
        )
        .unwrap();
        let mut vm = PortableVm::new(program);
        let result = vm.run();
        assert!(result.is_err(), "expected uncaught THROW to error");
    }

    #[test]
    fn test_portable_enter_try_and_exit_try_no_error() {
        // try { push 1 } catch { push 2 }
        // No throw occurs; EXIT_TRY should pop the handler and fall through
        // to `done:`. Stack must equal `[1]`.
        let source = r#"ENTER_TRY handler
PUSH 1
EXIT_TRY
JMP done
handler:
PUSH 2
done:
HALT"#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack, vec![Value::Int(1)]);
    }

    #[test]
    fn test_portable_try_catch_catches_throw() {
        // try { throw "err" } catch { pop error, push 99 }
        // After THROW the error value is pushed onto the stack and the ip
        // jumps to handler:. Catch handler pops the error and pushes 99.
        let source = r#"ENTER_TRY handler
PUSH_STR "err"
THROW
EXIT_TRY
JMP done
handler:
POP
PUSH 99
done:
HALT"#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack, vec![Value::Int(99)]);
    }

    #[test]
    fn test_portable_throw_error_value_on_stack_in_handler() {
        // try { throw "msg" } catch { the error value is already on stack }
        // The handler exits with HALT, leaving "msg" on the stack.
        let source = r#"ENTER_TRY handler
PUSH_STR "msg"
THROW
EXIT_TRY
JMP done
handler:
HALT
done:
HALT"#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        assert_eq!(r.stack, vec![Value::Str("msg".to_string())]);
    }

    #[test]
    fn test_portable_arr_push_and_arr_pop() {
        // Build [1, 2] using ARR_PUSH: each push leaves the array on the
        // stack (DUP-first). Final array on the stack should hold [1,2].
        let source = r#"NEW_ARRAY 0
    DUP
    PUSH 1
    ARR_PUSH
    DUP
    PUSH 2
    ARR_PUSH
    HALT"#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        let last = r.stack.last().expect("should have a value");
        match last {
            Value::Array(arr) => {
                let borrowed = arr.borrow();
                assert_eq!(borrowed.len(), 2);
                assert_eq!(borrowed[0], Value::Int(1));
                assert_eq!(borrowed[1], Value::Int(2));
            }
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn test_portable_arr_pop_removes_last() {
        // Build [1, 2, 3], pop once (yields 3 + [1,2]), pop once more
        // (yields 2 + [1]). Stack top is 2; second-to-top is [1].
        let source = r#"NEW_ARRAY 0
    DUP
    PUSH 1
    ARR_PUSH
    DUP
    PUSH 2
    ARR_PUSH
    DUP
    PUSH 3
    ARR_PUSH
    ARR_POP
    POP
    ARR_POP
    HALT"#;
        let program = assemble(source, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let r = vm.run().unwrap();
        let len = r.stack.len();
        assert!(len >= 2, "expected at least 2 values, got {len}");
        match &r.stack[len - 1] {
            Value::Int(v) => assert_eq!(*v, 2),
            other => panic!("expected Int(2), got {other:?}"),
        }
        match &r.stack[len - 2] {
            Value::Array(arr) => {
                let borrowed = arr.borrow();
                assert_eq!(borrowed.len(), 1);
                assert_eq!(borrowed[0], Value::Int(1));
            }
            other => panic!("expected Array([1]), got {other:?}"),
        }
    }
    #[test]
    fn test_portable_exec_lang_partial_binding() {
        // EXEC_LANG with var_count=3 but only var_0 is named.
        // Pop-on-name should pop 1 value (for var_0); unconditional pop would
        // pop 3 and crash with StackUnderflow.
        let spec = serde_json::json!({
            "lang": "bash",
            "code": "echo -n $FOO",
            "var_count": 3,
            "var_0": "FOO",
        });
        let src = format!(
            "PUSH_STR \"hello\"\nEXEC_LANG \"{}\"\nHALT",
            spec.to_string().replace('"', "\\\"")
        );
        let program = assemble(&src, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let mut caps = crate::HostCaps::new();
        caps.grant_polyglot(&["bash"]);
        vm.set_host_caps(caps);
        let result = vm.run().unwrap();
        assert_eq!(result.output, "hello");
        assert!(result.halted);
    }

    #[test]
    fn test_portable_exec_lang_all_named() {
        // Common path: var_count == number of named slots (k == N).
        let spec = serde_json::json!({
            "lang": "bash",
            "code": "echo -n ${X}${Y}",
            "var_count": 2,
            "var_0": "X",
            "var_1": "Y",
        });
        let src = format!(
            "PUSH_STR \"ab\"\nPUSH_STR \"AB\"\nEXEC_LANG \"{}\"\nHALT",
            spec.to_string().replace('"', "\\\"")
        );
        let program = assemble(&src, None, Some("test")).unwrap();
        let mut vm = PortableVm::new(program);
        let mut caps = crate::HostCaps::new();
        caps.grant_polyglot(&["bash"]);
        vm.set_host_caps(caps);
        let result = vm.run().unwrap();
        assert_eq!(result.output, "abAB");
        assert!(result.halted);
    }
}
