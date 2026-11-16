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
    Conformance, Const, Construct, CosmModule, Equality, FieldDecl, FnRef, HookImpl, Hooks,
    IGNORABLE_MEMBER_KEYS, InterfaceDecl, InterfaceMember, InterfaceRef, KindDecl, KindRef, Member,
    MemberDecl, Op, OwnerId, Param, Receiver, Resolution, Satisfaction, Semantics, StaticValue, Storage,
    ValueMode, owner_id_problem,
};
pub use span::{Diagnostic, DiagnosticCode, Span};
pub use types::CosmType;
pub use validate::{validate, validate_with};

/// Runs the README's examples as doctests.
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;

/// The newest COSM schema version this crate reads (and can write).
///
/// # Evolution rule (F7; decisions 5 and 7 of
/// `docs/design/visibility-and-packages.md`, 2026-11-14)
///
/// - COSM is **strict by default**: an unknown key anywhere is a load error
///   (`deny_unknown_fields`, and for [`MemberDecl`], which is flattened, an
///   explicit key check). An old reader refuses rather than guessing, so a
///   security-relevant field (visibility first) can never be silently dropped.
/// - A module may carry `cosm_version`. Absent means [`COSM_BASE_VERSION`]. A
///   reader accepts the **same major** and a **minor no greater than its own**
///   ([`check_readable`]); the patch is ignored. This holds for 0.x too.
/// - A new field or strategy **bumps the minor** of [`COSM_VERSION`]. The writer
///   stamps the **lowest version that can express the module**
///   ([`CosmModule::required_version`]), and omits `cosm_version` at the base
///   version, so a program that uses no newer feature loads on an old runtime
///   byte for byte. CVM1 adds a second, by-name gate: such a module is written
///   with kind-table format 2, which runtimes from before this rule refuse.
/// - A purely additive or cosmetic field may be declared ignorable (it still
///   bumps the minor). Nothing is ignorable today.
///
/// # Versions
///
/// - `0.1.0` ([`COSM_BASE_VERSION`]): the schema before the rule.
/// - `0.2.0` ([`COSM_OWNERS_VERSION`], V3b #522): owner ids. A [`KindDecl`] may
///   carry `module` and `package`, and the module may carry `fn_packages`, the
///   owning package of each function outside the root package. All three are
///   must-understand.
pub const COSM_VERSION: &str = "0.2.0";

/// The first version with owner ids (`KindDecl.module` / `KindDecl.package`
/// and `CosmModule.fn_packages`, V3b #522). A module that uses any of them is
/// stamped with it.
pub const COSM_OWNERS_VERSION: &str = "0.2.0";

/// The version an absent `cosm_version` means: the schema as it was before
/// the evolution rule.
pub const COSM_BASE_VERSION: &str = "0.1.0";

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
    let (a, b, c) = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some((a, b, c))
}

/// Can this crate read a module stamped `cosm_version = v`? Yes when `v` has
/// [`COSM_VERSION`]'s major and a minor no greater than its own. The error
/// names both versions.
pub fn check_readable(v: &str) -> Result<(), String> {
    check_readable_by(v, COSM_VERSION)
}

/// [`check_readable`] for a reader whose newest version is `reader`: the same
/// gate, so a test can ask what a reader of an earlier version (e.g.
/// [`COSM_BASE_VERSION`]) does with a stamp written today.
pub fn check_readable_by(v: &str, reader: &str) -> Result<(), String> {
    let Some((major, minor, _)) = parse_version(v) else {
        return Err(format!(
            "cosm_version `{v}` is not a MAJOR.MINOR.PATCH version"
        ));
    };
    let Some((own_major, own_minor, _)) = parse_version(reader) else {
        return Err(format!(
            "reader version `{reader}` is not a MAJOR.MINOR.PATCH version"
        ));
    };
    if major == own_major && minor <= own_minor {
        Ok(())
    } else {
        Err(format!(
            "this module needs COSM {v}, but this runtime reads COSM up to {reader} \
             (same major, minor no greater); use a toolchain that reads COSM {v}, or recompile \
             the program with this one"
        ))
    }
}

/// The package id an absent owner means: the program's own package (the
/// entry file's). It is never written: an owner field or `fn_packages` entry
/// is omitted for it, and a reader refuses it spelled out, so each package
/// has one spelling.
pub const ROOT_PACKAGE: &str = "root";

/// Errors reading COSM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CosmError {
    /// The JSON doesn't match the schema; `path` is where (e.g. `kinds[0].storage`).
    Json { path: String, message: String },
}

impl std::fmt::Display for CosmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CosmError::Json { path, message } => {
                write!(f, "invalid COSM JSON at `{path}`: {message}")
            }
        }
    }
}

impl std::error::Error for CosmError {}
