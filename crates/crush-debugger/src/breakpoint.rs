//! Breakpoint registry keyed by `<file>:<line>`.
//!
//! Breakpoints carry an optional `bytecode_address` resolved from
//! the assembler sourcemap at add-time via `DebugSession`. When set,
//! the VM driver forwards the address to `PortableVm` so the VM
//! pauses at the matching instruction pointer.
//!
//! `BreakpointId` is a monotonic counter assigned in `add()` order
//! so the REPL `delete <id>` command can refer to a single instance
//! without ambiguity (multiple BPs at the same location get distinct
//! IDs).

use indexmap::IndexMap;
use std::path::PathBuf;

/// A source-level breakpoint target. `file` is the resolved-on-disk
/// absolute or relative path that matches what the parser has; `line`
/// is 1-indexed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    pub file: PathBuf,
    pub line: u32,
}

/// Monotonic ID assigned in `add()` order. Useful for stable REPL
/// references ("delete 2") and for the driver's `find_by_id` future use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BreakpointId(pub u32);

/// Where a breakpoint actually lives in source. The SCAFFOLD only
/// carries the source location; the bytecode coord is `None` until
/// `crush-frontend` ships a sourcemap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint {
    pub id: BreakpointId,
    pub location: Location,
    /// Bytecode offset resolved from the assembler sourcemap at add-time.
    /// `None` if the sourcemap didn't contain a matching line (for programs
    /// assembled without debug info).
    pub bytecode_address: Option<u32>,
}

/// Breakpoint registry. Backed by an `IndexMap` keyed on
/// `(location, monotonic id)` to keep `add`/`remove` O(log N) AND
/// preserve insertion order for `list()`.
#[derive(Debug, Default, Clone)]
pub struct BreakpointSet {
    by_location: IndexMap<(Location, BreakpointId), Breakpoint>,
    next_id: u32,
}

impl BreakpointSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source-level breakpoint. Multiple breakpoints at the
    /// same location get distinct IDs (so `delete <id>` is unambiguous).
    /// If `bytecode_address` is `Some`, the VM will pause when the
    /// instruction pointer reaches that offset.
    /// Returns the assigned ID.
    pub fn add(
        &mut self,
        file: impl Into<PathBuf>,
        line: u32,
        bytecode_address: Option<u32>,
    ) -> BreakpointId {
        let id = BreakpointId(self.next_id);
        self.next_id += 1;
        let location = Location {
            file: file.into(),
            line,
        };
        self.by_location.insert(
            (location.clone(), id),
            Breakpoint {
                id,
                location,
                bytecode_address,
            },
        );
        id
    }

    /// Remove by ID. Returns `true` if the breakpoint was registered.
    pub fn remove(&mut self, id: BreakpointId) -> bool {
        let key = self
            .by_location
            .keys()
            .find(|(_, bid)| *bid == id)
            .cloned();
        match key {
            Some(k) => {
                self.by_location.remove(&k);
                true
            }
            None => false,
        }
    }

    /// All registered breakpoints in insertion order.
    pub fn list(&self) -> Vec<&Breakpoint> {
        self.by_location.values().collect()
    }

    /// Total count.
    pub fn len(&self) -> usize {
        self.by_location.len()
    }

    /// Empty check.
    pub fn is_empty(&self) -> bool {
        self.by_location.is_empty()
    }

    /// Does the registry contain a breakpoint at this exact location
    /// (any ID)?
    pub fn matches(&self, file: &std::path::Path, line: u32) -> bool {
        self.by_location
            .keys()
.any(|(loc, _)| loc.file == file && loc.line == line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_assigns_distinct_ids_for_same_location() {
        let mut set = BreakpointSet::new();
        let a = set.add("a.crush", 3, None);
        let b = set.add("a.crush", 3, None);
        assert_ne!(a, b);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn remove_returns_true_for_existing_false_for_missing() {
        let mut set = BreakpointSet::new();
        let id = set.add("a.crush", 3, None);
        assert!(set.remove(id));
        assert!(!set.remove(id));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn list_yields_insertion_order() {
        let mut set = BreakpointSet::new();
        let a = set.add("a.crush", 1, None);
        let b = set.add("b.crush", 7, None);
        let c = set.add("a.crush", 5, None);
        let ids: Vec<BreakpointId> = set.list().into_iter().map(|bp| bp.id).collect();
        assert_eq!(ids, vec![a, b, c]);
    }

    #[test]
    fn matches_returns_true_only_for_exact_file_line() {
        let mut set = BreakpointSet::new();
        set.add("a.crush", 3, None);
        assert!(set.matches(std::path::Path::new("a.crush"), 3));
        assert!(!set.matches(std::path::Path::new("a.crush"), 4));
        assert!(!set.matches(std::path::Path::new("b.crush"), 3));
    }
}
