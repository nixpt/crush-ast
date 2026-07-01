//! Runtime context shared between JIT-compiled code and the host.
//!
//! The [`JitContext`] is passed as a pointer argument to every JIT-compiled function.
//! Field offsets are fixed (ABI-stable) so the compiler can emit known offsets.
//!
//! # Layout (64-bit systems)
//!
//! | Offset | Size | Field |
//! |--------|------|-------|
//! | 0      | 8192 | stack[1024] |
//! | 8192   | 8    | stack_top |
//! | 8200   | 512  | locals[64] |
//! | 8712   | 8    | n_locals |
//! | 8720   | 8    | result |
//! | 8728   | 8    | budget |
//! | 8736   | 4    | error |
//! | 8740   | 4    | _pad |
//! | 8744   | 8    | arena |
//! | 8752   | 8    | capabilities |
//! | 8760   | 8    | hal |
//! | 8768   | —    | total |

use crate::value::{JitValue, TAG_NULL};
use std::ffi::c_void;

/// Maximum number of stack slots before spilling.
pub const JIT_STACK_SIZE: usize = 1024;

/// Maximum number of local variables.
pub const JIT_MAX_LOCALS: usize = 64;

/// ABI-stable context for JIT-compiled functions.
///
/// DO NOT reorder fields without updating [`crate::compiler`]'s offset constants.
#[repr(C)]
pub struct JitContext {
    /// Value stack (inline array for fast JIT access).
    pub stack: [JitValue; JIT_STACK_SIZE],
    /// Current stack pointer (next free slot index).
    pub stack_top: usize,
    /// Local variables.
    pub locals: [JitValue; JIT_MAX_LOCALS],
    /// Number of active locals.
    pub n_locals: usize,
    /// Result value (set on Halt).
    pub result: JitValue,
    /// Budget remaining.
    pub budget: u64,
    /// Error flag (non-zero = error).
    pub error: i32,
    /// Alignment padding.
    _pad: i32,
    /// Heap arena (opaque pointer).
    pub arena: *mut c_void,
    /// Capability table (opaque pointer).
    pub capabilities: *mut c_void,
    /// Host HAL (opaque pointer).
    pub hal: *mut c_void,
}

// ── Ensure layout is as expected ────────────────────────────────────────────

const _: () = {
    assert!(core::mem::size_of::<JitContext>() == 8768);
    assert!(core::mem::align_of::<JitContext>() == 8);
    assert!(core::mem::offset_of!(JitContext, stack) == 0);
    assert!(core::mem::offset_of!(JitContext, stack_top) == 8192);
    assert!(core::mem::offset_of!(JitContext, locals) == 8200);
    assert!(core::mem::offset_of!(JitContext, result) == 8720);
    assert!(core::mem::offset_of!(JitContext, budget) == 8728);
    assert!(core::mem::offset_of!(JitContext, error) == 8736);
};

impl JitContext {
    /// Create a new context with all fields zeroed/null.
    pub fn new() -> Self {
        Self {
            stack: [JitValue(TAG_NULL); JIT_STACK_SIZE],
            stack_top: 0,
            locals: [JitValue(TAG_NULL); JIT_MAX_LOCALS],
            n_locals: 0,
            result: JitValue(TAG_NULL),
            budget: 1_000_000,
            error: 0,
            _pad: 0,
            arena: std::ptr::null_mut(),
            capabilities: std::ptr::null_mut(),
            hal: std::ptr::null_mut(),
        }
    }

    /// Push a value onto the shadow stack.
    #[inline]
    pub fn push(&mut self, val: JitValue) {
        debug_assert!(self.stack_top < JIT_STACK_SIZE, "JIT stack overflow");
        self.stack[self.stack_top] = val;
        self.stack_top += 1;
    }

    /// Pop a value from the shadow stack.
    #[inline]
    pub fn pop(&mut self) -> Option<JitValue> {
        if self.stack_top == 0 {
            None
        } else {
            self.stack_top -= 1;
            Some(self.stack[self.stack_top])
        }
    }

    /// Peek at the top of the stack.
    #[inline]
    pub fn top(&self) -> Option<JitValue> {
        if self.stack_top == 0 {
            None
        } else {
            Some(self.stack[self.stack_top - 1])
        }
    }

    /// Read the result value.
    #[inline]
    pub fn result(&self) -> JitValue {
        self.result
    }

    /// Store the result value.
    #[inline]
    pub fn set_result(&mut self, val: JitValue) {
        self.result = val;
    }
}

impl Default for JitContext {
    fn default() -> Self {
        Self::new()
    }
}
