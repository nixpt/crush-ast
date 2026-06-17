//! CVM1 interpreter — sandboxed execution with hard quotas.
//!
//! The only way out of the sandbox is `CAP_CALL`. The program must declare
//! each cap in `manifest.permissions`; the host can further restrict via
//! `Quotas::allowed_caps`. Division and modulo truncate toward zero (matching
//! Python's `int(a/b)` for same-sign and `a//b` for same-sign).
//!
//! Array and map values use `Rc<RefCell<...>>` (shared reference semantics):
//! cloning a `Value::Array` or `Value::Map` produces an alias, not a copy.
//! This matches Python/JS list/dict behavior — a DUP followed by ARR_SET
//! mutates the same underlying storage as the original.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::bytecode::{
    self, ADD, AND, ARR_GET, ARR_LEN, ARR_POP, ARR_PUSH, ARR_SET, BITAND, BITNOT, BITOR, BITXOR,
    CALL, CAP_CALL, CAST, DIV, DUP, ENTER_TRY, EQ, EXEC_LANG, EXIT_TRY, GE, GET_FIELD, GT, HALT, JMP,
    JNZ, JZ, LE, LOAD, LT, MAKE_RANGE, MOD, MUL, NE, NEG, NEW_ARRAY, NEW_OBJ, NOP, NOT, OR, PICK, POP,
    PRINT, PUSH, PUSH_BOOL, PUSH_F64, PUSH_NULL, PUSH_STR, Program, RET, SET_FIELD, SHL, SHR,
    ROT, ROLL, SPAWN, STORE, STR_CONTAINS, STR_JOIN, STR_REPLACE, STR_SPLIT, SUB, SWAP, THROW, TYPEOF,
    YIELD, AWAIT,
};
use crate::caps::capabilities;
use crate::host::HostCaps;

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("stack underflow")]
    StackUnderflow,
    #[error("stack quota exceeded ({0})")]
    StackQuota(usize),
    #[error("instruction quota exceeded ({0})")]
    StepQuota(usize),
    #[error("output quota exceeded ({0})")]
    OutputQuota(usize),
    #[error("call depth quota exceeded ({0})")]
    CallDepthQuota(usize),
    #[error("unknown opcode {0:#04x} at {1}")]
    UnknownOpcode(u8, usize),
    #[error("truncated instruction at {0}")]
    TruncatedInstruction(usize),
    #[error("const index out of range: {0}")]
    ConstOutOfRange(usize),
    #[error("load from uninitialised slot {0}")]
    UninitSlot(u16),
    #[error("jump target {0} out of range")]
    BadJump(usize),
    #[error("call to unknown function: {0}")]
    UnknownFunction(String),
    #[error("type error: expected {expected}, got {got}")]
    TypeError {
        expected: &'static str,
        got: &'static str,
    },
    #[error("array index out of range: {index} (len {len})")]
    ArrayBounds { index: i64, len: usize },
    #[error("array index must be int, got {0}")]
    BadIndex(&'static str),
    #[error("division by zero")]
    DivByZero,
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    #[error("capability not declared in manifest: {0}")]
    CapNotDeclared(String),
    #[error("capability denied by host: {0}")]
    CapDenied(String),
    #[error("unknown capability: {0}")]
    UnknownCap(String),
    #[error("{cap} takes {expected} arg(s), got {got}")]
    CapArity {
        cap: String,
        expected: usize,
        got: usize,
    },
}

/// Stack value — the types the CVM1 supports.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    /// Shared array (reference semantics via Rc<RefCell<...>>).
    Array(std::rc::Rc<std::cell::RefCell<Vec<Value>>>),
    /// Shared string-keyed map (reference semantics via Rc<RefCell<...>>).
    Map(std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Value>>>),
    /// Error value (carries a message string).
    Error(String),
    /// Binary blob data.
    Bytes(Vec<u8>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => *a.borrow() == *b.borrow(),
            (Value::Map(a), Value::Map(b)) => *a.borrow() == *b.borrow(),
            (Value::Error(a), Value::Error(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
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

    pub(crate) fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Array(a) => !a.borrow().is_empty(),
            Value::Map(m) => !m.borrow().is_empty(),
            Value::Error(_) => true,
            Value::Bytes(b) => !b.is_empty(),
        }
    }

    pub(crate) fn as_text(&self) -> String {
        match self {
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
                let inner: Vec<_> = a.borrow().iter().map(|v| v.as_text()).collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Map(m) => {
                let inner: Vec<_> = m
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.as_text()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Value::Error(e) => format!("error({e})"),
            Value::Bytes(b) => format!("<{} bytes>", b.len()),
        }
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }

    pub fn new_array(v: Vec<Value>) -> Self {
        Value::Array(std::rc::Rc::new(std::cell::RefCell::new(v)))
    }

    pub fn new_map(m: std::collections::HashMap<String, Value>) -> Self {
        Value::Map(std::rc::Rc::new(std::cell::RefCell::new(m)))
    }
}

/// Execution resource limits.
#[derive(Debug, Clone)]
pub struct Quotas {
    pub max_steps: usize,
    pub max_stack: usize,
    pub max_output: usize,
    pub max_call_depth: usize,
    /// If set, further restricts the program's declared permissions.
    pub allowed_caps: Option<Vec<String>>,
}

impl Default for Quotas {
    fn default() -> Self {
        Self {
            max_steps: 1_000_000,
            max_stack: 4096,
            max_output: 1 << 20,
            max_call_depth: 256,
            allowed_caps: None,
        }
    }
}

/// Result of a successful run (no VmError).
#[derive(Debug, Default)]
pub struct VmResult {
    pub output: String,
    pub steps: usize,
    pub halted: bool,
    pub stack: Vec<Value>,
}

struct Frame {
    return_ip: Option<usize>,
    memory: HashMap<u16, Value>,
}

/// Run a program with the built-in portable capability registry only.
pub fn run(program: &Program, quotas: &Quotas) -> Result<VmResult, VmError> {
    run_with_caps(program, quotas, None)
}

/// Run a program with optional host-provided capabilities.
pub fn run_with_caps(
    program: &Program,
    quotas: &Quotas,
    host_caps: Option<&HostCaps>,
) -> Result<VmResult, VmError> {
    let code = &program.code;
    let n = code.len();
    let mut stack: Vec<Value> = Vec::new();
    let mut out_parts: Vec<String> = Vec::new();
    let mut out_len: usize = 0;
    let declared: std::collections::HashSet<&str> = program
        .manifest
        .permissions
        .iter()
        .map(|s| s.as_str())
        .collect();
    let mut steps: usize = 0;

    let func_entry: HashMap<&str, usize> = program
        .manifest
        .functions
        .iter()
        .map(|(k, v)| (k.as_str(), v.entry))
        .collect();

    let start_ip = program
        .manifest
        .entry
        .as_deref()
        .and_then(|e| func_entry.get(e).copied())
        .unwrap_or_else(|| func_entry.values().copied().next().unwrap_or(0));

    let mut ip = start_ip;
    let mut call_stack: Vec<Frame> = vec![Frame { return_ip: None, memory: HashMap::new() }];
    let mut try_stack: Vec<usize> = Vec::new();

    // Async task infrastructure: spawned functions stored by ID, executed on await
    let mut next_task_id: u64 = 1;
    let mut scheduled_tasks: HashMap<u64, String> = HashMap::new();

    macro_rules! pop {
        () => {{ stack.pop().ok_or(VmError::StackUnderflow)? }};
    }
    macro_rules! push {
        ($v:expr) => {{
            if stack.len() >= quotas.max_stack {
                return Err(VmError::StackQuota(quotas.max_stack));
            }
            stack.push($v);
        }};
    }
    macro_rules! need_num {
        ($v:expr) => {{
            let v = $v;
            if !v.is_numeric() {
                return Err(VmError::TypeError {
                    expected: "numeric",
                    got: v.type_name(),
                });
            }
            v
        }};
    }

    while ip < n {
        steps += 1;
        if steps > quotas.max_steps {
            return Err(VmError::StepQuota(quotas.max_steps));
        }
        let opcode = code[ip];
        let isize = bytecode::instruction_size(opcode).ok_or(VmError::UnknownOpcode(opcode, ip))?;
        if ip + isize > n {
            return Err(VmError::TruncatedInstruction(ip));
        }
        let next_ip = ip + isize;

        match opcode {
            NOP => {}
            PUSH => {
                let v = i64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
                push!(Value::Int(v));
            }
            PUSH_F64 => {
                let v = f64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
                push!(Value::Float(v));
            }
            PUSH_BOOL => {
                let v = i64::from_be_bytes(code[ip + 1..ip + 9].try_into().unwrap());
                push!(Value::Bool(v != 0));
            }
            PUSH_NULL => {
                push!(Value::Null);
            }
            PUSH_STR => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let s = program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?;
                push!(Value::Str(s.clone()));
            }
            POP => {
                pop!();
            }
            DUP => {
                let v = pop!();
                push!(v.clone());
                push!(v);
            }
            SWAP => {
                let a = pop!();
                let b = pop!();
                push!(a);
                push!(b);
            }
            ROT => {
                let a = pop!();
                let b = pop!();
                let c = pop!();
                push!(b);
                push!(c);
                push!(a);
            }
            PICK => {
                let n = u16::from_be_bytes(code[ip+1..ip+3].try_into().unwrap()) as usize;
                let len = stack.len();
                if n >= len { return Err(VmError::StackUnderflow); }
                push!(stack[len - 1 - n].clone());
            }
            ROLL => {
                let n = u16::from_be_bytes(code[ip+1..ip+3].try_into().unwrap()) as usize;
                let len = stack.len();
                if n >= len { return Err(VmError::StackUnderflow); }
                let idx = len - 1 - n;
                let v = stack.remove(idx);
                push!(v);
            }

            EQ => {
                let b = pop!();
                let a = pop!();
                push!(Value::Bool(a == b));
            }
            NE => {
                let b = pop!();
                let a = pop!();
                push!(Value::Bool(a != b));
            }
            ADD | SUB | MUL | DIV | MOD | LT | GT | LE | GE => {
                let b = need_num!(pop!());
                let a = need_num!(pop!());
                let is_float = matches!((&a, &b), (Value::Float(_), _) | (_, Value::Float(_)));
                let af = to_f64(&a);
                let bf = to_f64(&b);
                let result = match opcode {
                    ADD => {
                        if is_float {
                            Value::Float(af + bf)
                        } else {
                            Value::Int(
                                to_i64(&a)
                                    .checked_add(to_i64(&b))
                                    .ok_or(VmError::ArithmeticOverflow)?,
                            )
                        }
                    }
                    SUB => {
                        if is_float {
                            Value::Float(af - bf)
                        } else {
                            Value::Int(
                                to_i64(&a)
                                    .checked_sub(to_i64(&b))
                                    .ok_or(VmError::ArithmeticOverflow)?,
                            )
                        }
                    }
                    MUL => {
                        if is_float {
                            Value::Float(af * bf)
                        } else {
                            Value::Int(
                                to_i64(&a)
                                    .checked_mul(to_i64(&b))
                                    .ok_or(VmError::ArithmeticOverflow)?,
                            )
                        }
                    }
                    DIV => {
                        if bf == 0.0 {
                            return Err(VmError::DivByZero);
                        }
                        if is_float {
                            Value::Float(af / bf)
                        } else {
                            let ai = to_i64(&a);
                            let bi = to_i64(&b);
                            Value::Int(trunc_div(ai, bi))
                        }
                    }
                    MOD => {
                        if bf == 0.0 {
                            return Err(VmError::DivByZero);
                        }
                        if is_float {
                            Value::Float(af % bf)
                        } else {
                            let ai = to_i64(&a);
                            let bi = to_i64(&b);
                            Value::Int(ai - bi * trunc_div(ai, bi))
                        }
                    }
                    LT => Value::Bool(af < bf),
                    GT => Value::Bool(af > bf),
                    LE => Value::Bool(af <= bf),
                    GE => Value::Bool(af >= bf),
                    _ => unreachable!(),
                };
                push!(result);
            }
            NEG => {
                let v = need_num!(pop!());
                push!(match v {
                    Value::Int(i) => Value::Int(-i),
                    Value::Float(f) => Value::Float(-f),
                    _ => unreachable!(),
                });
            }
            AND | OR => {
                let b = pop!();
                let a = pop!();
                push!(match opcode {
                    AND => Value::Bool(a.is_truthy() && b.is_truthy()),
                    OR => Value::Bool(a.is_truthy() || b.is_truthy()),
                    _ => unreachable!(),
                });
            }
            BITAND | BITOR | BITXOR | SHL | SHR => {
                let b = need_num!(pop!());
                let a = need_num!(pop!());
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
                push!(result);
            }
            BITNOT => {
                let a = need_num!(pop!());
                push!(Value::Int(!to_i64(&a)));
            }
            NOT => {
                let v = pop!();
                push!(Value::Bool(!v.is_truthy()));
            }
            TYPEOF => {
                let v = pop!();
                push!(Value::Str(v.type_name().to_string()));
            }
            CAST => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let type_name = program.consts.get(idx).ok_or(VmError::ConstOutOfRange(idx))?.clone();
                let v = pop!();
                match type_name.as_str() {
                    "str" | "string" => push!(Value::Str(v.as_text())),
                    "int" | "i64" => match v {
                        Value::Int(_) => push!(v),
                        Value::Float(f) => push!(Value::Int(f as i64)),
                        Value::Str(s) => push!(Value::Int(s.parse().unwrap_or(0))),
                        Value::Bool(b) => push!(Value::Int(if b { 1 } else { 0 })),
                        _ => push!(Value::Int(0)),
                    },
                    "float" | "f64" => match v {
                        Value::Float(_) => push!(v),
                        Value::Int(i) => push!(Value::Float(i as f64)),
                        Value::Str(s) => push!(Value::Float(s.parse().unwrap_or(0.0))),
                        Value::Bool(b) => push!(Value::Float(if b { 1.0 } else { 0.0 })),
                        _ => push!(Value::Float(0.0)),
                    },
                    "bool" => push!(Value::Bool(v.is_truthy())),
                    _ => push!(v),
                }
            }
            NEW_ARRAY => {
                let count = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    vals.push(pop!());
                }
                vals.reverse();
                push!(Value::new_array(vals));
            }
            ARR_GET => {
                let idx_v = pop!();
                let arr_v = pop!();
                let idx = need_array_index(&idx_v)?;
                let arr_rc = need_array(arr_v)?;
                let arr = arr_rc.borrow();
                let len = arr.len();
                let actual = wrap_index(idx, len)?;
                push!(arr[actual].clone());
            }
            ARR_SET => {
                let val = pop!();
                let idx_v = pop!();
                let arr_v = pop!();
                let idx = need_array_index(&idx_v)?;
                let arr_rc = need_array(arr_v)?;
                {
                    let mut arr = arr_rc.borrow_mut();
                    let len = arr.len();
                    let actual = wrap_index(idx, len)?;
                    arr[actual] = val;
                }
                push!(Value::Array(arr_rc));
            }
            ARR_LEN => {
                let v = pop!();
                let arr_rc = need_array(v)?;
                let len = arr_rc.borrow().len();
                push!(Value::Int(len as i64));
            }
            ARR_PUSH => {
                let val = pop!();
                let arr_rc = need_array(pop!())?;
                arr_rc.borrow_mut().push(val);
                push!(Value::Array(arr_rc));
            }
            ARR_POP => {
                let arr_rc = need_array(pop!())?;
                let val = arr_rc.borrow_mut().pop().unwrap_or(Value::Null);
                push!(Value::Array(arr_rc.clone()));
                push!(val);
            }
            LOAD => {
                let slot = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap());
                let v = call_stack
                    .last()
                    .unwrap()
                    .memory
                    .get(&slot)
                    .ok_or(VmError::UninitSlot(slot))?
                    .clone();
                push!(v);
            }
            STORE => {
                let slot = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap());
                let v = pop!();
                call_stack.last_mut().unwrap().memory.insert(slot, v);
            }
            JMP | JZ | JNZ => {
                let target = u32::from_be_bytes(code[ip + 1..ip + 5].try_into().unwrap()) as usize;
                if target > n {
                    return Err(VmError::BadJump(target));
                }
                let take = match opcode {
                    JMP => true,
                    JZ => !pop!().is_truthy(),
                    JNZ => pop!().is_truthy(),
                    _ => unreachable!(),
                };
                if take {
                    ip = target;
                    continue;
                }
            }
            PRINT => {
                let s = pop!().as_text();
                out_len += s.len();
                if out_len > quotas.max_output {
                    return Err(VmError::OutputQuota(quotas.max_output));
                }
                out_parts.push(s);
            }
            CAP_CALL => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let argc = code[ip + 3] as usize;
                let cap = program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    args.push(pop!());
                }
                args.reverse();
                let result = dispatch_cap(
                    &cap,
                    args,
                    &declared,
                    quotas,
                    &mut out_parts,
                    &mut out_len,
                    host_caps,
                )?;
                if let Some(v) = result {
                    push!(v);
                }
            }
            CALL => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let fname = program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?;
                let entry = func_entry
                    .get(fname.as_str())
                    .copied()
                    .ok_or_else(|| VmError::UnknownFunction(fname.clone()))?;
                if call_stack.len() >= quotas.max_call_depth {
                    return Err(VmError::CallDepthQuota(quotas.max_call_depth));
                }
                call_stack.push(Frame {
                    return_ip: Some(next_ip),
                    memory: HashMap::new(),
                });
                ip = entry;
                continue;
            }
            RET => {
                let frame = call_stack.pop().expect("call stack invariant");
                match frame.return_ip {
                    None => {
                        return Ok(VmResult {
                            output: out_parts.concat(),
                            steps,
                            halted: true,
                            stack,
                        });
                    }
                    Some(ret) => {
                        ip = ret;
                        continue;
                    }
                }
            }
            EXEC_LANG => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let spec_json = program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let spec: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_str(&spec_json).map_err(|_| {
                        VmError::UnknownCap("exec_lang: invalid args JSON".to_string())
                    })?;
                let lang = spec.get("lang").and_then(|v| v.as_str()).unwrap_or("?");
                let code_str = spec.get("code").and_then(|v| v.as_str()).unwrap_or("");
                let var_count =
                    spec.get("var_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                // Collect variable names from spec
                let mut var_names: Vec<String> = Vec::with_capacity(var_count);
                let mut var_values: Vec<Value> = Vec::with_capacity(var_count);
                for i in 0..var_count {
                    let key = format!("var_{}", i);
                    if let Some(name) = spec.get(&key).and_then(|v| v.as_str()) {
                        var_names.push(name.to_string());
                        // Pop the value that was pushed by the load instruction
                        var_values.push(pop!());
                    }
                }
                // Reverse so values correspond to names in order
                var_values.reverse();
                let mut cmd = std::process::Command::new(lang);
                cmd.arg("-c").arg(code_str);
                for (name, val) in var_names.iter().zip(var_values.iter()) {
                    cmd.env(name, val.as_text());
                }
                let output = cmd
                    .output()
                    .map_err(|e| VmError::UnknownCap(format!("exec_lang({lang}): {e}")))?;
                if output.status.success() {
                    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    out_len += s.len();
                    if out_len > quotas.max_output {
                        return Err(VmError::OutputQuota(quotas.max_output));
                    }
                    out_parts.push(s.clone());
                    // Push output back onto stack for the store instruction
                    push!(Value::Str(s));
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    return Err(VmError::UnknownCap(format!("exec_lang({lang}): {err}")));
                }
            }
            ENTER_TRY => {
                let target = u32::from_be_bytes(code[ip + 1..ip + 5].try_into().unwrap()) as usize;
                if target > n {
                    return Err(VmError::BadJump(target));
                }
                try_stack.push(target);
            }
            EXIT_TRY => {
                try_stack.pop();
            }
            THROW => {
                let err_val = pop!();
                if let Some(handler_ip) = try_stack.pop() {
                    ip = handler_ip;
                    push!(err_val);
                    continue;
                }
                return Err(VmError::UnknownCap(format!(
                    "uncaught error: {}",
                    err_val.as_text()
                )));
            }
            STR_CONTAINS => {
                let needle = pop!();
                let haystack = pop!();
                push!(Value::Bool(haystack.as_text().contains(&needle.as_text())));
            }
            STR_SPLIT => {
                let delim = pop!();
                let s = pop!();
                let text = s.as_text();
                let d = delim.as_text();
                let parts: Vec<Value> = if d.is_empty() {
                    text.chars().map(|c| Value::Str(c.to_string())).collect()
                } else {
                    text.split(&d).map(|p| Value::Str(p.to_string())).collect()
                };
                push!(Value::new_array(parts));
            }
            STR_REPLACE => {
                let to = pop!();
                let from = pop!();
                let s = pop!();
                push!(Value::Str(
                    s.as_text().replace(&from.as_text(), &to.as_text())
                ));
            }
            STR_JOIN => {
                let delim = pop!();
                let arr_v = pop!();
                let d = delim.as_text();
                match arr_v {
                    Value::Array(elems) => {
                        let parts: Vec<String> = elems.borrow().iter().map(|v| v.as_text()).collect();
                        push!(Value::Str(parts.join(&d)));
                    }
                    other => {
                        return Err(VmError::TypeError {
                            expected: "array",
                            got: other.type_name(),
                        });
                    }
                }
            }
            MAKE_RANGE => {
                let end_v = pop!();
                let start_v = pop!();
                let start = match start_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: other.type_name(),
                        });
                    }
                };
                let end = match end_v {
                    Value::Int(i) => i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: other.type_name(),
                        });
                    }
                };
                let mut elems = Vec::new();
                if start < end {
                    for i in start..end {
                        elems.push(Value::Int(i));
                    }
                }
                push!(Value::new_array(elems));
            }
            NEW_OBJ => {
                push!(Value::new_map(std::collections::HashMap::new()));
            }
            SET_FIELD => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let field = program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let val = pop!();
                let map_rc = match pop!() {
                    Value::Map(m) => m,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "map",
                            got: other.type_name(),
                        });
                    }
                };
                map_rc.borrow_mut().insert(field, val);
                push!(Value::Map(map_rc));
            }
            GET_FIELD => {
                let idx = u16::from_be_bytes(code[ip + 1..ip + 3].try_into().unwrap()) as usize;
                let field = program
                    .consts
                    .get(idx)
                    .ok_or(VmError::ConstOutOfRange(idx))?
                    .clone();
                let map_rc = match pop!() {
                    Value::Map(m) => m,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "map",
                            got: other.type_name(),
                        });
                    }
                };
                let val = map_rc.borrow().get(&field).cloned().unwrap_or(Value::Null);
                push!(val);
            }
            SPAWN => {
                let fn_name = pop!().as_text();
                let task_id = next_task_id;
                next_task_id += 1;
                scheduled_tasks.insert(task_id, fn_name);
                push!(Value::Int(task_id as i64));
            }
            YIELD => {
                std::thread::yield_now();
            }
            AWAIT => {
                let handle = pop!();
                if let Value::Int(task_id) = handle {
                    if let Some(fn_name) = scheduled_tasks.remove(&(task_id as u64)) {
                        // Run the scheduled task synchronously (like CALL then continue)
                        if let Some(&entry) = func_entry.get(fn_name.as_str()) {
                            if call_stack.len() >= quotas.max_call_depth {
                                return Err(VmError::CallDepthQuota(quotas.max_call_depth));
                            }
                            call_stack.push(Frame {
                                return_ip: Some(next_ip),
                                memory: HashMap::new(),
                            });
                            ip = entry;
                            continue;
                        }
                    }
                }
                // Non-task handle or unknown task: push null as result
                push!(Value::Null);
            }
            HALT => {
                return Ok(VmResult {
                    output: out_parts.concat(),
                    steps,
                    halted: true,
                    stack,
                });
            }
            _ => return Err(VmError::UnknownOpcode(opcode, ip)),
        }
        ip = next_ip;
    }

    Ok(VmResult {
        output: out_parts.concat(),
        steps,
        halted: false,
        stack,
    })
}

fn dispatch_cap(
    cap: &str,
    args: Vec<Value>,
    declared: &std::collections::HashSet<&str>,
    quotas: &Quotas,
    out_parts: &mut Vec<String>,
    out_len: &mut usize,
    host_caps: Option<&HostCaps>,
) -> Result<Option<Value>, VmError> {
    if !declared.contains(cap) {
        return Err(VmError::CapNotDeclared(cap.to_string()));
    }
    if let Some(allowed) = &quotas.allowed_caps
        && !allowed.iter().any(|a| a == cap)
    {
        return Err(VmError::CapDenied(cap.to_string()));
    }

    // Built-in portable capabilities.
    if let Some(spec) = capabilities().get(cap) {
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
                let s: String = args
                    .iter()
                    .map(|a| a.as_text())
                    .collect::<Vec<_>>()
                    .concat();
                *out_len += s.len();
                if *out_len > quotas.max_output {
                    return Err(VmError::OutputQuota(quotas.max_output));
                }
                out_parts.push(s);
                Ok(None)
            }
            "str.concat" => {
                let s: String = args
                    .iter()
                    .map(|a| a.as_text())
                    .collect::<Vec<_>>()
                    .concat();
                Ok(Some(Value::Str(s)))
            }
            "str.len" => {
                let s = args[0].as_text();
                Ok(Some(Value::Int(s.len() as i64)))
            }
            "str.contains" => {
                let haystack = args[0].as_text();
                let needle = args[1].as_text();
                Ok(Some(Value::Bool(haystack.contains(&needle))))
            }
            "str.split" => {
                let s = args[0].as_text();
                let delim = args[1].as_text();
                let parts: Vec<Value> = if delim.is_empty() {
                    s.chars().map(|c| Value::Str(c.to_string())).collect()
                } else {
                    s.split(&delim).map(|p| Value::Str(p.to_string())).collect()
                };
                Ok(Some(Value::new_array(parts)))
            }
            "str.replace" => {
                let s = args[0].as_text();
                let from = args[1].as_text();
                let to = args[2].as_text();
                Ok(Some(Value::Str(s.replace(&from, &to))))
            }
            "str.join" => {
                let delim = args[1].as_text();
                match &args[0] {
                    Value::Array(elems) => {
                        let parts: Vec<String> = elems.borrow().iter().map(|v| v.as_text()).collect();
                        Ok(Some(Value::Str(parts.join(&delim))))
                    }
                    other => Err(VmError::TypeError {
                        expected: "array",
                        got: other.type_name(),
                    }),
                }
            }
            "make_range" => {
                let start = match &args[0] {
                    Value::Int(i) => *i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: other.type_name(),
                        });
                    }
                };
                let end = match &args[1] {
                    Value::Int(i) => *i,
                    other => {
                        return Err(VmError::TypeError {
                            expected: "int",
                            got: other.type_name(),
                        });
                    }
                };
                let mut elems = Vec::new();
                if start < end {
                    for i in start..end {
                        elems.push(Value::Int(i));
                    }
                }
                Ok(Some(Value::new_array(elems)))
            }
            _ => Err(VmError::UnknownCap(cap.to_string())),
        };
    }

    // Host-provided capabilities.
    if let Some(host) = host_caps
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

#[inline]
fn to_f64(v: &Value) -> f64 {
    match v {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

#[inline]
fn to_i64(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i64,
        _ => 0,
    }
}

/// Truncate toward zero (matches Python `int(a/b)` for opposite-sign operands).
#[inline]
fn trunc_div(a: i64, b: i64) -> i64 {
    a / b // Rust integer division already truncates toward zero
}

fn need_array(v: Value) -> Result<std::rc::Rc<std::cell::RefCell<Vec<Value>>>, VmError> {
    match v {
        Value::Array(a) => Ok(a),
        other => Err(VmError::TypeError {
            expected: "array",
            got: other.type_name(),
        }),
    }
}

fn need_array_index(v: &Value) -> Result<i64, VmError> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(VmError::BadIndex(other.type_name())),
    }
}

fn wrap_index(idx: i64, len: usize) -> Result<usize, VmError> {
    let ilen = len as i64;
    if idx >= -ilen && idx < ilen {
        Ok(if idx < 0 {
            (ilen + idx) as usize
        } else {
            idx as usize
        })
    } else {
        Err(VmError::ArrayBounds { index: idx, len })
    }
}
