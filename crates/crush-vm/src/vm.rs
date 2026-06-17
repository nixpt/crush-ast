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

use crate::bytecode::Program;
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
    /// Green thread handle — returned by spawn, consumed by await.
    Handle(u64),
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
            (Value::Handle(a), Value::Handle(b)) => a == b,
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
            Value::Handle(_) => "handle",
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
            Value::Handle(_) => true,
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
            Value::Handle(id) => format!("<handle {}>", id),
        }
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }
}

/// Per-thread execution state for the green-thread scheduler.
pub struct GreenThread {
    pub ip: usize,
    pub stack: Vec<Value>,
    pub call_stack: Vec<Frame>,
    pub try_stack: Vec<usize>,
    pub steps: usize,
    pub done: bool,
    pub yielded: bool,
    pub waiting_for: Option<u64>,
    pub return_value: Option<Value>,
    pub out_parts: Vec<String>,
    pub out_len: usize,
}

impl GreenThread {
    pub fn new(ip: usize) -> Self {
        Self {
            ip,
            stack: Vec::new(),
            call_stack: vec![Frame { return_ip: None, memory: HashMap::new() }],
            try_stack: Vec::new(),
            steps: 0,
            done: false,
            yielded: false,
            waiting_for: None,
            return_value: None,
            out_parts: Vec::new(),
            out_len: 0,
        }
    }
}

impl Value {
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

pub struct Frame {
    pub return_ip: Option<usize>,
    pub memory: HashMap<u16, Value>,
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
    crate::scheduler::run_scheduled(program, quotas, host_caps)
}

