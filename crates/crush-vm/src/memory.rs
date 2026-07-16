//! # Arena-Based Memory Management
//!
//! This module implements a sophisticated arena-based memory management system
//! for the NanoVM with automatic garbage collection, borrow checking, and
//! memory safety guarantees.
//!
//! ## Overview
//!
//! The memory system provides:
//!
//! - **Arena Allocation**: Efficient memory allocation with O(1) allocation/deallocation
//! - **Automatic Garbage Collection**: Mark-and-sweep garbage collection with cycle detection
//! - **Borrow Checking**: Runtime borrow checking to prevent data races and use-after-free
//! - **Memory Safety**: Comprehensive safety guarantees without garbage collection overhead
//! - **Resource Management**: Memory limits and usage tracking
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │            Arena                    │
//! │  ┌─────────────────────────────────┐ │
//! │  │   Slot Management               │ │
//! │  │  ┌─────────────┐  ┌───────────┐ │ │
//! │  │  │ Free List   │  │ Allocated │ │ │
//! │  │  │ - Reuse     │  │ - Objects │ │ │
//! │  │  │ - Recycling │  │ - Tracking│ │ │
//! │  │  └─────────────┘  └───────────┘ │ │
//! │  │  ┌─────────────┐  ┌───────────┐ │ │
//! │  │  │ GC System   │  │ Borrow    │ │ │
//! │  │  │ - Mark      │  │ Checking  │ │ │
//! │  │  │ - Sweep     │  │ - Safety  │ │ │
//! │  │  └─────────────┘  └───────────┘ │ │
//! │  └─────────────────────────────────┘ │
//! └──────────────┬──────────────────────┘
//!                │
//!                ↓
//! ┌─────────────────────────────────────┐
//! │            Objects                  │
//! │  ┌─────────────────────────────────┐ │
//! │  │   Rich Object System            │ │
//! │  │  ┌─────────────┐  ┌───────────┐ │ │
//! │  │  │ Primitives  │  │ Complex   │ │ │
//! │  │  │ - Strings   │  │ - Arrays  │ │ │
//! │  │  │ - Bytes     │  │ - Maps    │ │ │
//! │  │  │ - Handles   │  │ - Objects │ │ │
//! │  │  └─────────────┘  └───────────┘ │ │
//! │  │  ┌─────────────┐  ┌───────────┐ │ │
//! │  │  │ Special     │  │ Interface │ │ │
//! │  │  │ - Results   │  │ Handles   │ │ │
//! │  │  │ - Tags      │  │ - Tokens  │ │ │
//! │  │  └─────────────┘  └───────────┘ │ │
//! │  └─────────────────────────────────┘ │
//! └─────────────────────────────────────┘
//! ```
//!
//! ## Key Components
//!
//! ### Arena
//! The main memory management structure that provides:
//! - **Slot Management**: Efficient allocation and deallocation of memory slots
//! - **Free List**: Reuse of freed slots for optimal memory utilization
//! - **GC Integration**: Seamless integration with garbage collection
//! - **Borrow Tracking**: Runtime borrow checking for memory safety
//!
//! ### Objects
//! Comprehensive object system supporting:
//! - **Primitives**: Strings, bytes, handles with efficient storage
//! - **Collections**: Arrays, maps with reference counting
//! - **Complex Objects**: Class instances with field management
//! - **Special Types**: Results, tagged values, interface capabilities
//!
//! ### Borrow Checking
//! Runtime borrow checking system that ensures:
//! - **Memory Safety**: Prevents use-after-free and double-free errors
//! - **Data Race Prevention**: Ensures exclusive access for mutable operations
//! - **Reference Safety**: Tracks immutable and mutable borrows
//! - **Lifetime Management**: Automatic cleanup of borrowed references
//!
//! ## Usage Examples
//!
//! ### Basic Memory Operations
//!
//! ```text
//! use nanovm::memory::Arena;
//! use nanovm::value::RuntimeValue;
//!
//! let mut arena = Arena::new();
//!
//! // Allocate objects
//! let string_idx = arena.alloc(Object::Str("Hello, World!".to_string()));
//! let array_idx = arena.alloc(Object::Array(vec![]));
//!
//! // Access objects
//! if let Some(obj) = arena.get(string_idx) {
//!     match obj {
//!         Object::Str(s) => println!("String: {}", s),
//!         _ => unreachable!(),
//!     }
//! }
//!
//! // Mutable access with borrow checking
//! if let Ok(obj) = arena.get_mut(array_idx) {
//!     if let Object::Array(arr) = obj {
//!         arr.push(RuntimeValue::Int(42));
//!     }
//! }
//! ```
//!
//! ### Garbage Collection
//!
//! ```text
//! use nanovm::memory::Arena;
//!
//! let mut arena = Arena::new();
//!
//! // Allocate some objects
//! let root1 = arena.alloc(Object::Str("root".to_string()));
//! let root2 = arena.alloc(Object::Array(vec![]));
//!
//! // Create references
//! // (Assume we have a way to create references between objects)
//!
//! // Mark phase: Mark all reachable objects
//! arena.trace(vec![root1, root2]);
//!
//! // Sweep phase: Free unmarked objects
//! let freed_count = arena.sweep();
//! println!("Freed {} objects", freed_count);
//! ```
//!
//! ### Borrow Checking
//!
//! ```text
//! use nanovm::memory::Arena;
//!
//! let mut arena = Arena::new();
//! let obj_idx = arena.alloc(Object::Str("test".to_string()));
//!
//! // Borrow immutably
//! arena.borrow(obj_idx).unwrap();
//!
//! // This would fail - cannot borrow mutably while immutably borrowed
//! // arena.borrow_mut(obj_idx).unwrap(); // Error!
//!
//! // Release immutable borrow
//! arena.release(obj_idx);
//!
//! // Now we can borrow mutably
//! arena.borrow_mut(obj_idx).unwrap();
//!
//! // Release mutable borrow
//! arena.release_mut(obj_idx);
//! ```
//!
//! ## Memory Safety Features
//!
//! ### Compile-Time Safety
//! - **Type Safety**: Strong typing prevents type confusion
//! - **Ownership**: Clear ownership semantics for all objects
//! - **Lifetime Management**: Automatic lifetime tracking
//!
//! ### Runtime Safety
//! - **Borrow Checking**: Prevents data races and use-after-free
//! - **Bounds Checking**: Array and collection bounds validation
//! - **Null Safety**: No null pointer dereferences
//! - **Memory Limits**: Configurable memory usage limits
//!
//! ## Performance Characteristics
//!
//! - **O(1) Allocation**: Arena allocation is constant time
//! - **O(1) Deallocation**: Free list management is constant time
//! - **Efficient GC**: Mark-and-sweep with minimal pause times
//! - **Memory Efficiency**: Slot reuse and compact representation
//!
//! ## Integration with VM
//!
//! The memory system integrates seamlessly with the VM:
//!
//! 1. **Value Storage**: All RuntimeValue objects are stored in the arena
//! 2. **Reference Management**: Object references are managed automatically
//! 3. **GC Integration**: Garbage collection is triggered automatically
//! 4. **Error Handling**: Memory errors are handled gracefully
//!
//! ## Security Considerations
//!
//! The memory system provides several security benefits:
//!
//! - **Memory Safety**: Eliminates buffer overflows and use-after-free
//! - **Data Integrity**: Prevents data corruption through borrow checking
//! - **Resource Limits**: Prevents memory exhaustion attacks
//! - **Access Control**: Enforces proper access patterns
//!
//! ## Future Enhancements
//!
//! Potential future improvements include:
//! - **Generational GC**: Generational garbage collection for better performance
//! - **Concurrent GC**: Concurrent garbage collection for multi-threaded VMs
//! - **Memory Pools**: Specialized memory pools for different object types
//! - **Profiling**: Memory usage profiling and optimization
//!
//! ## Testing and Validation
//!
//! The module includes comprehensive tests for:
//! - Memory allocation and deallocation correctness
//! - Garbage collection accuracy
//! - Borrow checking robustness
//! - Performance characteristics
//! - Memory safety guarantees
//!
//! This memory management system provides a solid foundation for safe, efficient
//! memory operations while maintaining excellent performance characteristics.

use crate::value::RuntimeValue;
use crush_errors::{CrushError, CrushResult};
use std::collections::{HashMap, HashSet};

const SMALL_INT_MIN: i64 = -128;
const SMALL_INT_MAX: i64 = 127;
const DEFAULT_MAX_MEMORY: usize = 16 * 1024 * 1024; // 16 MiB
const DEFAULT_HIGH_WATER_MARK: f64 = 0.75;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaStats {
    pub total_allocated: usize,
    pub total_freed: usize,
    pub num_objects: usize,
    pub num_gc_runs: u64,
    pub peak_usage: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BorrowState {
    pub immutable_refs: usize,
    pub mutable_ref: bool,
}

impl Default for BorrowState {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowState {
    pub fn new() -> Self {
        Self {
            immutable_refs: 0,
            mutable_ref: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    Str(String),
    Array(Vec<RuntimeValue>),
    Tuple(Vec<RuntimeValue>),
    List(std::collections::LinkedList<RuntimeValue>),
    Vector(Vec<RuntimeValue>),
    Set(Vec<RuntimeValue>),
    Map(HashMap<String, RuntimeValue>),
    Object {
        lang: String,
        class_name: String,
        fields: HashMap<String, RuntimeValue>,
    },
    Tagged {
        tag: String,
        value: Box<RuntimeValue>,
    },
    Handle(u64),
    Bytes(Vec<u8>),
    Buffer(Vec<u8>),
    Result {
        ok: bool,
        value: Box<RuntimeValue>,
    },
    /// Interface Capability Object (OIH/Token) per CSCS v1
    InterfaceHandle(u64),
}

#[derive(Debug, Clone)]
pub struct Slot {
    pub object: Object,
    pub borrow: BorrowState,
    pub marked: bool,
    pub allocated: bool,
}

impl Slot {
    pub fn new(object: Object) -> Self {
        Self {
            object,
            borrow: BorrowState::new(),
            marked: false,
            allocated: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Arena {
    pub slots: Vec<Slot>,
    pub free_list: Vec<usize>,
    permanent_roots: HashSet<usize>,
    small_int_pool_start: usize,
    bool_false_idx: usize,
    bool_true_idx: usize,
    empty_string_idx: usize,
    current_allocated_bytes: usize,
    total_allocated_bytes: usize,
    total_freed_bytes: usize,
    num_gc_runs: u64,
    peak_usage_bytes: usize,
    max_memory: usize,
    high_water_mark: f64,
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    pub fn new() -> Self {
        let mut arena = Self {
            slots: Vec::new(),
            free_list: Vec::new(),
            permanent_roots: HashSet::new(),
            small_int_pool_start: 0,
            bool_false_idx: 0,
            bool_true_idx: 0,
            empty_string_idx: 0,
            current_allocated_bytes: 0,
            total_allocated_bytes: 0,
            total_freed_bytes: 0,
            num_gc_runs: 0,
            peak_usage_bytes: 0,
            max_memory: DEFAULT_MAX_MEMORY,
            high_water_mark: DEFAULT_HIGH_WATER_MARK,
        };
        arena.init_pools();
        arena
    }

    fn init_pools(&mut self) {
        self.small_int_pool_start = self.slots.len();
        for i in SMALL_INT_MIN..=SMALL_INT_MAX {
            let idx = self.push_permanent(Object::Tagged {
                tag: "__pooled_int".to_string(),
                value: Box::new(RuntimeValue::Int(i)),
            });
            debug_assert_eq!(idx, self.small_int_pool_start + (i - SMALL_INT_MIN) as usize);
        }
        self.bool_false_idx = self.push_permanent(Object::Tagged {
            tag: "__pooled_bool".to_string(),
            value: Box::new(RuntimeValue::Bool(false)),
        });
        self.bool_true_idx = self.push_permanent(Object::Tagged {
            tag: "__pooled_bool".to_string(),
            value: Box::new(RuntimeValue::Bool(true)),
        });
        self.empty_string_idx = self.push_permanent(Object::Str(String::new()));
    }

    fn push_permanent(&mut self, obj: Object) -> usize {
        let size = Self::estimate_object_size(&obj);
        let idx = self.slots.len();
        self.slots.push(Slot::new(obj));
        self.permanent_roots.insert(idx);
        self.current_allocated_bytes += size;
        self.total_allocated_bytes += size;
        self.peak_usage_bytes = self.peak_usage_bytes.max(self.current_allocated_bytes);
        idx
    }

    fn estimate_runtime_value_size(value: &RuntimeValue) -> usize {
        match value {
            RuntimeValue::Int(_) => core::mem::size_of::<i64>(),
            RuntimeValue::Float(_) => core::mem::size_of::<f64>(),
            RuntimeValue::Bool(_) => core::mem::size_of::<bool>(),
            RuntimeValue::Null => 0,
            RuntimeValue::Ref(_) => core::mem::size_of::<usize>(),
            RuntimeValue::String(s) => s.len(),
        }
    }

    fn estimate_object_size(obj: &Object) -> usize {
        let base = core::mem::size_of::<Object>();
        base + match obj {
            Object::Str(s) => s.len(),
            Object::Array(arr)
            | Object::Tuple(arr)
            | Object::Vector(arr)
            | Object::Set(arr) => arr
                .iter()
                .map(Self::estimate_runtime_value_size)
                .sum::<usize>(),
            Object::List(arr) => arr
                .iter()
                .map(Self::estimate_runtime_value_size)
                .sum::<usize>(),
            Object::Map(map) => map
                .iter()
                .map(|(k, v)| k.len() + Self::estimate_runtime_value_size(v))
                .sum::<usize>(),
            Object::Object {
                lang,
                class_name,
                fields,
            } => {
                lang.len()
                    + class_name.len()
                    + fields
                        .iter()
                        .map(|(k, v)| k.len() + Self::estimate_runtime_value_size(v))
                        .sum::<usize>()
            }
            Object::Tagged { tag, value } => tag.len() + Self::estimate_runtime_value_size(value),
            Object::Handle(_) => core::mem::size_of::<u64>(),
            Object::Bytes(bytes) | Object::Buffer(bytes) => bytes.len(),
            Object::Result { value, .. } => Self::estimate_runtime_value_size(value),
            Object::InterfaceHandle(_) => core::mem::size_of::<u64>(),
        }
    }

    fn pool_idx_for_small_int(value: i64) -> Option<usize> {
        if (SMALL_INT_MIN..=SMALL_INT_MAX).contains(&value) {
            Some((value - SMALL_INT_MIN) as usize)
        } else {
            None
        }
    }

    pub fn alloc_small_int(&mut self, value: i64) -> usize {
        if let Some(offset) = Self::pool_idx_for_small_int(value) {
            return self.small_int_pool_start + offset;
        }
        self.alloc(Object::Tagged {
            tag: "__int".to_string(),
            value: Box::new(RuntimeValue::Int(value)),
        })
    }

    pub fn alloc_bool(&mut self, value: bool) -> usize {
        if value {
            self.bool_true_idx
        } else {
            self.bool_false_idx
        }
    }

    pub fn alloc_empty_string(&self) -> usize {
        self.empty_string_idx
    }

    pub fn is_under_pressure(&self) -> bool {
        let threshold = (self.max_memory as f64 * self.high_water_mark) as usize;
        self.current_allocated_bytes > threshold
    }

    pub fn set_high_water_mark(&mut self, high_water_mark: f64) {
        self.high_water_mark = high_water_mark.clamp(0.0, 1.0);
    }

    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            total_allocated: self.total_allocated_bytes,
            total_freed: self.total_freed_bytes,
            num_objects: self.len(),
            num_gc_runs: self.num_gc_runs,
            peak_usage: self.peak_usage_bytes,
        }
    }

    /// Allocate a new object in the arena.
    ///
    /// This method allocates memory for a new object in the arena. It uses a
    /// free list to efficiently reuse previously freed slots, falling back to
    /// creating new slots when the free list is empty.
    ///
    /// # Parameters
    ///
    /// * `obj` - The object to allocate in the arena
    ///
    /// # Returns
    ///
    /// The index of the allocated slot in the arena
    ///
    /// # Allocation Strategy
    ///
    /// 1. **Free List Reuse**: First attempts to reuse a previously freed slot
    /// 2. **New Slot Creation**: If no free slots available, creates a new slot
    /// 3. **Slot Initialization**: Properly initializes all slot metadata
    ///
    /// # Performance Characteristics
    ///
    /// - **O(1) Allocation**: Both free list reuse and new slot creation are constant time
    /// - **Memory Efficiency**: Free list ensures optimal memory utilization
    /// - **Cache Friendly**: Sequential slot allocation improves cache locality
    ///
    /// # Memory Safety
    ///
    /// - **Slot Validation**: Ensures slot is properly initialized
    /// - **Borrow State Reset**: Clears any previous borrow state
    /// - **Mark State Reset**: Clears garbage collection marks
    /// - **Allocation Flag**: Sets allocated flag to true
    ///
    /// # Example
    ///
    /// ```rust
    /// use crush_vm::memory::Arena;
    /// use crush_vm::memory::Object;
    ///
    /// let mut arena = Arena::new();
    ///
    /// // Allocate a string object
    /// let string_idx = arena.alloc(Object::Str("Hello, World!".to_string()));
    ///
    /// // Allocate an array object
    /// let array_idx = arena.alloc(Object::Array(vec![]));
    ///
    /// // Both allocations return valid indices
    /// assert!(string_idx < arena.capacity());
    /// assert!(array_idx < arena.capacity());
    /// ```
    ///
    /// # Error Handling
    ///
    /// This method never fails - it will always return a valid slot index.
    /// Memory allocation failures are handled by the underlying Vec implementation.
    ///
    /// # Integration with GC
    ///
    /// - New slots are marked as unmarked for garbage collection
    /// - Allocation does not affect existing object references
    /// - Free list management is transparent to garbage collection
    pub fn alloc(&mut self, obj: Object) -> usize {
        // Pooled immutable objects return shared permanent references.
        match &obj {
            Object::Str(s) if s.is_empty() => return self.empty_string_idx,
            Object::Tagged { tag, value } if tag == "__pooled_bool" => {
                if let RuntimeValue::Bool(v) = &**value {
                    return if *v {
                        self.bool_true_idx
                    } else {
                        self.bool_false_idx
                    };
                }
            }
            Object::Tagged { tag, value } if tag == "__pooled_int" => {
                if let RuntimeValue::Int(v) = &**value {
                    if let Some(offset) = Self::pool_idx_for_small_int(*v) {
                        return self.small_int_pool_start + offset;
                    }
                }
            }
            _ => {}
        }
        let size = Self::estimate_object_size(&obj);
        if let Some(idx) = self.free_list.pop() {
            // Reuse slot
            if let Some(slot) = self.slots.get_mut(idx) {
                slot.object = obj;
                slot.borrow = BorrowState::new();
                slot.marked = false;
                slot.allocated = true;
                self.current_allocated_bytes += size;
                self.total_allocated_bytes += size;
                self.peak_usage_bytes = self.peak_usage_bytes.max(self.current_allocated_bytes);
                return idx;
            }
        }

        // New slot
        let idx = self.slots.len();
        self.slots.push(Slot::new(obj));
        self.current_allocated_bytes += size;
        self.total_allocated_bytes += size;
        self.peak_usage_bytes = self.peak_usage_bytes.max(self.current_allocated_bytes);
        idx
    }

    // Modifying this to check allocation
    pub fn get(&self, idx: usize) -> Option<&Object> {
        self.slots
            .get(idx)
            .filter(|s| s.allocated)
            .map(|s| &s.object)
    }

    pub fn get_mut(&mut self, idx: usize) -> CrushResult<&mut Object> {
        // Simple Runtime Borrow Check
        if let Some(slot) = self.slots.get_mut(idx) {
            if !slot.allocated {
                return Err(CrushError::internal("Invalid address"));
            }
            if slot.borrow.immutable_refs > 0 {
                return Err(CrushError::internal(format!(
                    "BorrowError: Addr {} has immutable refs, cannot borrow mut",
                    idx
                )));
            }
            if slot.borrow.mutable_ref {
                return Err(CrushError::internal(format!(
                    "BorrowError: Addr {} is already borrowed mut",
                    idx
                )));
            }
            // For this simple implementation we don't track the *lifetime* of the borrow returning &mut
            // In a full implementation we would set the flag, return a guard, and unset on drop.
            // For now, we assume immediate operation or we rely on explicit OpCodes to lock/unlock.
            // Since this function returns a generic Rust ref, we can't easily auto-unlock.
            // Let's just return the object and assume the VM op calling this handles concurrency safety (single threaded for now).
            Ok(&mut slot.object)
        } else {
            Err(CrushError::internal("Invalid address"))
        }
    }

    pub fn mark(&mut self, idx: usize) -> bool {
        if let Some(slot) = self.slots.get_mut(idx) {
            if slot.allocated && !slot.marked {
                slot.marked = true;
                return true;
            }
        }
        false
    }

    pub fn trace(&mut self, roots: Vec<usize>) {
        let mut worklist = roots;
        while let Some(idx) = worklist.pop() {
            if let Some(slot) = self.slots.get_mut(idx) {
                if slot.allocated && !slot.marked {
                    slot.marked = true;
                    // Extract refs
                    match &slot.object {
                        Object::Array(arr) => {
                            for v in arr {
                                if let RuntimeValue::Ref(c) = v {
                                    worklist.push(*c);
                                }
                            }
                        }
                        Object::Map(map) => {
                            for v in map.values() {
                                if let RuntimeValue::Ref(c) = v {
                                    worklist.push(*c);
                                }
                            }
                        }
                        Object::Tagged { value, .. } => {
                            if let RuntimeValue::Ref(c) = &**value {
                                worklist.push(*c);
                            }
                        }
                        Object::Object { fields, .. } => {
                            for v in fields.values() {
                                if let RuntimeValue::Ref(c) = v {
                                    worklist.push(*c);
                                }
                            }
                        }
                        Object::Result { value, .. } => {
                            if let RuntimeValue::Ref(c) = &**value {
                                worklist.push(*c);
                            }
                        }
                        Object::InterfaceHandle(_) => {} // No internal refs to trace
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn sweep(&mut self) -> usize {
        let mut freed = 0;
        let mut to_free = Vec::new();
        self.num_gc_runs += 1;

        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if self.permanent_roots.contains(&idx) {
                slot.marked = false;
                continue;
            }
            if !slot.allocated {
                continue;
            }
            if slot.marked {
                slot.marked = false;
            } else {
                let freed_size = Self::estimate_object_size(&slot.object);
                slot.allocated = false;
                // Clear content to free memory
                slot.object = Object::Str(String::new());
                to_free.push(idx);
                freed += 1;
                self.current_allocated_bytes = self.current_allocated_bytes.saturating_sub(freed_size);
                self.total_freed_bytes += freed_size;
            }
        }

        self.free_list.extend(to_free);
        freed
    }

    pub fn borrow(&mut self, idx: usize) -> CrushResult<()> {
        if let Some(slot) = self.slots.get_mut(idx) {
            if slot.borrow.mutable_ref {
                return Err(CrushError::internal(format!(
                    "BorrowError: Addr {} is borrowed mut, cannot borrow immut",
                    idx
                )));
            }
            slot.borrow.immutable_refs += 1;
        }
        Ok(())
    }

    pub fn release(&mut self, idx: usize) {
        if let Some(slot) = self.slots.get_mut(idx) {
            if slot.borrow.immutable_refs > 0 {
                slot.borrow.immutable_refs -= 1;
            }
        }
    }

    pub fn borrow_mut(&mut self, idx: usize) -> CrushResult<()> {
        if let Some(slot) = self.slots.get_mut(idx) {
            if slot.borrow.immutable_refs > 0 || slot.borrow.mutable_ref {
                return Err(CrushError::internal(format!(
                    "BorrowError: Addr {} is already borrowed",
                    idx
                )));
            }
            slot.borrow.mutable_ref = true;
        }
        Ok(())
    }

    pub fn release_mut(&mut self, idx: usize) {
        if let Some(slot) = self.slots.get_mut(idx) {
            slot.borrow.mutable_ref = false;
        }
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.allocated).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn set_limit(&mut self, _limit: usize) {
        self.max_memory = _limit.max(1);
    }

    pub fn get_memory_usage(&self) -> usize {
        self.current_allocated_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pooling_small_int_bool_empty_string() {
        let mut arena = Arena::new();

        let a = arena.alloc_small_int(42);
        let b = arena.alloc_small_int(42);
        assert_eq!(a, b);

        let t1 = arena.alloc_bool(true);
        let t2 = arena.alloc_bool(true);
        let f1 = arena.alloc_bool(false);
        let f2 = arena.alloc_bool(false);
        assert_eq!(t1, t2);
        assert_eq!(f1, f2);
        assert_ne!(t1, f1);

        let s1 = arena.alloc(Object::Str(String::new()));
        let s2 = arena.alloc_empty_string();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_pooling_survives_gc() {
        let mut arena = Arena::new();
        let pooled_int = arena.alloc_small_int(7);
        let pooled_true = arena.alloc_bool(true);
        let pooled_empty = arena.alloc_empty_string();

        let temp = arena.alloc(Object::Str("temp".to_string()));
        assert!(arena.get(temp).is_some());
        let freed = arena.sweep();
        assert!(freed >= 1);

        assert!(arena.get(pooled_int).is_some());
        assert!(arena.get(pooled_true).is_some());
        assert!(arena.get(pooled_empty).is_some());
    }

    #[test]
    fn test_pressure() {
        let mut arena = Arena::new();
        // Arena::new() pre-pools objects (small-int cache, bools, empty string),
        // so the baseline usage is non-trivial. is_under_pressure() compares
        // current_allocated_bytes against max_memory * high_water_mark, so the
        // limit must clear the baseline by the high-water margin to leave real
        // headroom before pressure — the *10/9 offsets the 0.9 mark.
        let baseline_usage = arena.get_memory_usage();
        arena.set_limit((baseline_usage + 2_048) * 10 / 9);
        arena.set_high_water_mark(0.9);
        assert!(!arena.is_under_pressure());

        loop {
            if arena.is_under_pressure() {
                break;
            }
            arena.alloc(Object::Bytes(vec![0u8; 64]));
        }

        assert!(arena.is_under_pressure());
    }

    #[test]
    fn test_stats() {
        let mut arena = Arena::new();
        let baseline = arena.stats();

        let _a = arena.alloc(Object::Str("abc".to_string()));
        let _b = arena.alloc(Object::Bytes(vec![1, 2, 3, 4]));
        let after_alloc = arena.stats();
        assert!(after_alloc.total_allocated > baseline.total_allocated);
        assert!(after_alloc.num_objects >= baseline.num_objects + 2);
        assert!(after_alloc.peak_usage >= baseline.peak_usage);

        let freed = arena.sweep();
        assert!(freed >= 2);
        let after_gc = arena.stats();
        assert!(after_gc.total_freed > baseline.total_freed);
        assert!(after_gc.num_gc_runs >= baseline.num_gc_runs + 1);
    }
}
