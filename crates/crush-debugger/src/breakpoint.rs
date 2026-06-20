//! Breakpoint registry keyed by `<file>:<line>`.
//!
//! The SCAFFOLD registers a breakpoint as a `Location` (resolved path
//! + 1-indexed line). Storing a `BreakpointId` per add() lets a future
//! REPL `delete <id>` command refer to a single instance without
//! ambiguity (multiple BPs at the same location get distinct IDs).
//!
//! The bytecode-address mapping is intentionally `None` today; it
//! will land alongside `crush-frontend`'s sourcemap.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// A source-level breakpoint target. `file` is the resolved-on-disk
/// absolute or relative path that matches what the parser has; `line`
/// is 1-indexed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Filled by `crush-frontend` once a sourcemap is emitted.
    /// TODO(DEBUGGER-2): populate via `compile_crush_source` sourcemap.
    pub bytecode_address: Option<u32>,
}

/// Breakpoint registry. Backed by a `BTreeMap` keyed on
/// `(location, monotonic id)` to keep `add`/`remove` O(log N) AND
/// preserve insertion order for `list()`.
#[derive(Debug, Default, Clone)]
pub struct BreakpointSet {
    by_location: BTreeMap<(Location, BreakpointId), Breakpoint>,
    next_id: u32,
}

impl BreakpointSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source-level breakpoint. Multiple breakpoints at the
    /// same location get distinct IDs (so `delete <id>` is unambiguous).
    /// Returns the assigned ID.
    pub fn add(&mut self, file: impl Into<PathBuf>, line: u32) -> BreakpointId {
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
                bytecode_address: None,
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
        let a = set.add("a.crush", 3);
        let b = set.add("a.crush", 3);
        assert_ne!(a, b);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn remove_returns_true_for_existing_false_for_missing() {
        let mut set = BreakpointSet::new();
        let id = set.add("a.crush", 3);
        assert!(set.remove(id));
        assert!(!set.remove(id));
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn list_yields_sorted_by_location() {
        let mut set = BreakpointSet::new();
        let a = set.add("a.crush", 1);
        let b = set.add("b.crush", 7);
        let c = set.add("a.crush", 5);
        let ids: Vec<BreakpointId> = set.list().into_iter().map(|bp| bp.id).collect();
        assert_eq!(ids, vec![a, b, c]);
    }

    #[test]
    fn matches_returns_true_only_for_exact_file_line() {
        let mut set = BreakpointSet::new();
        set.add("a.crush", 3);
        assert!(set.matches(std::path::Path::new("a.crush"), 3));
        assert!(!set.matches(std::path::Path::new("a.crush"), 4));
        assert!(!set.matches(std::path::Path::new("b.crush"), 3));
    }
}
