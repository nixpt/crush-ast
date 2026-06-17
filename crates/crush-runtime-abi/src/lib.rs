//! Guest Runtime ABI — interface for hosting foreign language VMs
//! under Crush's capability model.
//!
//! A "guest runtime" is a language VM (RustPython, QuickJS, etc.)
//! that runs inside CrushVM as a sandboxed capsule. It cannot access
//! host resources directly — all I/O goes through Crush capabilities.

use std::sync::Arc;

use anyhow::Result;

// ── Value types ─────────────────────────────────────────────────────────────

/// A value exchanged between Crush and a guest runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum GuestValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<GuestValue>),
    Map(std::collections::HashMap<String, GuestValue>),
    Bytes(Vec<u8>),
    /// An opaque handle to a guest-side object (e.g. a Python function).
    Opaque(GuestHandle),
}

/// Handle to a guest-side object that cannot be directly represented.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GuestHandle(pub u64);

/// Capability identifier string.
pub type CapabilityName = String;

// ── Runtime limits ──────────────────────────────────────────────────────────

/// Resource limits for a guest runtime invocation.
#[derive(Debug, Clone)]
pub struct RuntimeLimits {
    pub max_memory_bytes: u64,
    pub max_steps: u64,
    pub max_wall_time_ms: u64,
    pub allow_imports: bool,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 50 * 1024 * 1024,  // 50 MB
            max_steps: 1_000_000,
            max_wall_time_ms: 5_000,              // 5 seconds
            allow_imports: false,
        }
    }
}

// ── Guest context ───────────────────────────────────────────────────────────

/// The context in which a guest runtime executes.
pub struct GuestContext {
    pub granted_caps: Vec<CapabilityName>,
    pub limits: RuntimeLimits,
}

impl GuestContext {
    pub fn new(granted_caps: Vec<CapabilityName>) -> Self {
        Self {
            granted_caps,
            limits: RuntimeLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
        self.limits = limits;
        self
    }
}

// ── Guest Runtime trait ─────────────────────────────────────────────────────

/// A foreign language VM hosted inside CrushVM.
///
/// Implementations: RustPython, QuickJS, etc.
pub trait GuestRuntime {
    /// Evaluate Python/JS/... source code and return the result value.
    fn eval_source(&mut self, source: &str, ctx: &GuestContext) -> Result<GuestValue>;

    /// Call a named function defined in the guest runtime.
    fn call(&mut self, name: &str, args: &[GuestValue], ctx: &GuestContext) -> Result<GuestValue>;
}
