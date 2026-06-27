//! `crush-pkg` library facade.
//!
//! Exposes the runner, manifest, packer, signer, builder, and other
//! building blocks so embedded tests and downstream capsules can
//! `use crush_pkg::...` directly. The binary entry point stays in
//! `main.rs`, which retains CLI-shaped items only (`Cli`,
//! `Commands`, `MessageFormat`, failure-path routing,
//! `Code_{NEW,BUILDER,RUN,SIGN,SITE,MANIFEST,LINT}` constants,
//! the `mod tests` lockdown matrix, etc.).
//!
//! ## Why this exists
//!
//! `TICKETS/CRUSH-SELFHOST-1.md#constraint-4` said "crush-pkg is
//! currently binary-only" — integration tests couldn't `use
//! crush_pkg::runners::CrushRunner` and so drove the VM path
//! directly via `crush_lang_sdk::compile::compile_crush_source +
//! crush_vm::run_with_caps`, duplicating the body of
//! `CrushRunner::run` rather than going through it. The lib facade
//! is the seam that closes the gap: the binary's runtime code path
//! (`main.rs`) now imports the same modules as the integration
//! test (`tests/test_selfhost_demo.rs`), so a regression on
//! either side surfaces as a build error instead of a hidden
//! runtime drift.
//!
//! ## Boundary doctrine
//!
//! All structural code lives in the lib. CLI-shaped code stays in
//! `main.rs`. Modules referenced from `main.rs` were promoted to
//! `pub mod` here so `main.rs` can resolve `crush_pkg::foo::...`.
//! Internally-orphaned helpers stay as private `mod` (see below).
//!
//! Per the doctrine: `crush-pkg` is the **capsule pipeline** for
//! Crush — the lib exposes the pipeline's structural pieces
//! (load manifest, build program, register command, etc.) so the
//! exact same code path that the binary walks ships to anyone who
//! embeds `crush-pkg`.
//!
//! ## Curated re-exports
//!
//! Downstream capsules that embed `crush-pkg` reach the runner
//! triumvirate (`CapsuleRunner` trait + `CrushRunner` impl +
//! `ExecutionResult` enum) and the `Manifest` struct through a
//! stable short path at the lib root (e.g. `crush_pkg::CrushRunner`,
//! `crush_pkg::Manifest`), rather than spelling out the deeper
//! `crush_pkg::runners::CrushRunner`. **Adding to this set is the
//! canonical place to promote an item to "public API".** Internals
//! may refactor; the curated surface stays. Cross-bin wire
//! lockdowns stay at their respective modules (per the per-binary
//! wire-code doctrine in `main.rs`).
//!
//! Items NOT in the curated list are still reachable via the full
//! nested module path (`crush_pkg::manifest::CapsuleType`,
//! `crush_pkg::manifest::PayloadFormat`, etc.) — the curated set
//! is the **symbol of stable intent**, not a closure of all possible
//! access. Adding a new top-level re-export is a one-line change
//! HERE.

// ---------------------------------------------------------------
// Curated public surface — minimum-scope set
// ---------------------------------------------------------------
// The types the integration test `tests/test_selfhost_demo.rs`
// and downstream capsules reach through. Adding to this list is
// the canonical place to promote an item to "public API".
pub use manifest::Manifest;
pub use runners::{CapsuleRunner, CrushRunner, ExecutionResult};

// ---------------------------------------------------------------
// Public modules (CLI-reachable + integration-test-reachable)
// ---------------------------------------------------------------
// Module-level `//!` header doc lives in each source file
// (e.g. `src/manifest.rs`) and is the single source of doc truth
// — adding per-`pub mod` `///` here would surface BOTH in
// `cargo doc --open` and create a sync hazard. Keep this list
// terse.

pub mod builder;
pub mod ecap; // Underpins [`site`] for static-site capsules.
pub mod manifest;
pub mod packer;
pub mod runners; // [`CapsuleRunner`] triumvirate lives here.
pub mod signer;
pub mod site; // Static-site builder. Wraps [`ecap`].

// ---------------------------------------------------------------
// Internal helpers (private — not part of the API surface)
// ---------------------------------------------------------------
// Kept as `mod` rather than `pub mod` because nothing outside
// `crush-pkg` reaches them today and the previous crate-wide
// `#![allow(dead_code)]` silenced what would otherwise be a
// real dead_code warning on internal-only items (e.g.
// `merkle::compute_merkle_root` is `pub` but only invoked from
// its own `#[cfg(test)] mod`). Promote to `pub mod` ONLY when
// an external consumer actually needs it — this is the
// "downgrade path" round-2 reviewer recommended for orphaned
// modules. Settles both round-1 blocker #1 (over-broad allow)
// and round-2 blocker #3 (orphaned modules) at once.
mod bundle;
mod merkle;


