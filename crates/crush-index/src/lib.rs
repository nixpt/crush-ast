//! `crush-index` — Step 4 of the AI-native roadmap.
//!
//! Consumes annotated CAST programs and builds a queryable cross-reference index.
//!
//! The index answers the questions that cost agents hundreds of lines of file
//! reading today:
//! - `modules()` — workspace map; fits in ~20 context lines
//! - `definition("fn_name")` — signature + contracts, no source read needed
//! - `callers("fn_name")` — call sites across all indexed programs
//! - `invariants("module")` — what must stay true before touching a module
//! - `exhaustive_sites("TypeName")` — all match sites for a sum type
//! - `uncovered_paths()` — error paths with no `@covers` test
//!
//! Storage: in-memory `HashMap` for now.  A SQLite persistence layer (planned)
//! will be added in a later step without changing the query API.

pub mod index;
pub mod query;
pub mod stale;

pub use index::{CrushIndex, FunctionEntry, ModuleEntry};
pub use query::{CallSite, CoverageGap};

#[cfg(test)]
mod tests;
