//! Type definitions for FastVM execution.

use crate::value::RuntimeValue;
use std::collections::HashMap;

/// Execution result from FastVM
#[derive(Debug, Clone, PartialEq)]
pub enum FastYield {
    /// Completed successfully with optional return value
    Finished(Option<RuntimeValue>),
    /// Voluntary yield (cooperative multitasking)
    Yielded,
    /// Execution budget exhausted
    BudgetExhausted,
    /// Runtime error
    Error(FastError),
    /// Request to the host environment (syscall)
    Request(HostRequest),
    /// Yield with a value (used in some phases)
    Value(RuntimeValue),
}

/// Request for external action
#[derive(Debug, Clone, PartialEq)]
pub enum HostRequest {
    CallHost { 
        capsule_name: String, 
        method_name: String, 
        ic_id: [u8; 32], 
        args: Vec<RuntimeValue> 
    },
    CallInterface { 
        handle: RuntimeValue, 
        method_name: String, 
        args: Vec<RuntimeValue> 
    },
    ExecLang { 
        lang: String, 
        code: String, 
        variables: HashMap<String, RuntimeValue> 
    },
    Spawn { func: String },
    Restart { task_id: usize },
    Watchdog { task_id: usize, deadline: u64, action: String },
    Gc,
    ImportVar { name: String },
    ExportVar { name: String, value: RuntimeValue },
    Await { event_id: String },
}

impl FastYield {
    pub fn is_err(&self) -> bool {
        matches!(self, FastYield::Error(_))
    }
}

/// Minimal error type for fast path
#[derive(Debug, Clone, PartialEq)]
pub enum FastError {
    StackUnderflow,
    InvalidLocal(u32),
    InvalidCapability(u32),
    InvalidFunction(u32),
    TypeMismatch,
    DivisionByZero,
    Unimplemented(String),
    InternalError(String),
    ExecutionError(String),
    ResourceLimitExceeded,

    FunctionNotFound,
    InvalidSession,
    InvalidAgent,
}

impl std::fmt::Display for FastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FastError::StackUnderflow => write!(f, "Stack underflow"),
            FastError::InvalidLocal(i) => write!(f, "Invalid local index: {}", i),
            FastError::InvalidCapability(i) => write!(f, "Invalid capability index: {}", i),
            FastError::InvalidFunction(i) => write!(f, "Invalid function index: {}", i),
            FastError::TypeMismatch => write!(f, "Type mismatch"),
            FastError::DivisionByZero => write!(f, "Division by zero"),
            FastError::Unimplemented(msg) => write!(f, "Unimplemented: {}", msg),
            FastError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            FastError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            FastError::ResourceLimitExceeded => write!(f, "Resource limit exceeded"),
            FastError::FunctionNotFound => write!(f, "Function not found"),
            FastError::InvalidSession => write!(f, "Invalid session"),
            FastError::InvalidAgent => write!(f, "Invalid agent"),
        }
    }
}

/// Call frame for the fast VM
#[derive(Debug, Clone)]
pub struct FastFrame {
    pub return_pc: usize,
    pub locals_base: usize,  // Start index in locals vec
    #[allow(dead_code)]
    pub locals_count: usize, // Number of locals in this frame
    pub handlers: Vec<usize>, // Exception handler return addresses (PCs)
}
