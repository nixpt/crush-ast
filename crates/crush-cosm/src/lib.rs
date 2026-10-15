//! # crush-cosm — the Crush Object Semantic Model
//!
//! COSM is the lowered, declarative object model: the object-side sibling of
//! CAST. A walker lowers each language's object declarations (Rust
//! `struct`/`impl`/`trait`, Python `class`, JS `class`, Go types, Crush
//! `struct`/`impl`/`trait`) to [`KindDecl`]s and [`InterfaceDecl`]s, collected
//! in a [`CosmModule`]. What differs between languages is a choice from a
//! small, **closed** catalogue of strategies ([`Storage`], [`Semantics`],
//! [`Resolution`], [`Receiver`], [`Construct`], [`Satisfaction`],
//! [`Equality`]), each implemented once by the VM.
//!
//! This crate is pure data and algorithms:
//! - the schema, with serde JSON round-trip ([`CosmModule::from_json`],
//!   [`CosmModule::to_json`]);
//! - [`validate`] / [`validate_with`]: compile-time checks with span-carrying
//!   [`Diagnostic`]s;
//! - [`c3_mro`] (exactly CPython's C3, including its error cases and
//!   messages) and [`resolution_order`] for every linear strategy;
//! - [`catalogue`]: the textual spelling of every strategy, for `@kind(…)`.
//!
//! See `docs/design/object-model.md` for the design and the crate README for
//! the JSON shape.

pub mod catalogue;
pub mod mro;
pub mod schema;
pub mod span;
pub mod types;
mod validate;

pub use catalogue::{CatalogueError, Strategy};
pub use mro::{KindLookup, Linearizer, ModuleSet, MroError, c3_mro, resolution_order};
pub use schema::{
    Conformance, Const, Construct, CosmModule, Equality, FieldDecl, FnRef, HookImpl, Hooks, InterfaceDecl,
    InterfaceMember, InterfaceRef, KindDecl, KindRef, Member, MemberDecl, Op, Param, Receiver, Resolution,
    Satisfaction, Semantics, StaticValue, Storage, ValueMode,
};
pub use types::CosmType;
pub use span::{Diagnostic, DiagnosticCode, Span};
pub use validate::{validate, validate_with};

/// The COSM schema version this crate reads and writes.
pub const COSM_VERSION: &str = "0.1.0";

/// Errors reading COSM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CosmError {
    /// The JSON doesn't match the schema; `path` is where (e.g. `kinds[0].storage`).
    Json { path: String, message: String },
}

impl std::fmt::Display for CosmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CosmError::Json { path, message } => write!(f, "invalid COSM JSON at `{path}`: {message}"),
        }
    }
}

impl std::error::Error for CosmError {}
