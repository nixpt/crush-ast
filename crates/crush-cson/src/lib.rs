//! Crush VM host capability for CAISON — the `cson.parse` capability.
//!
//! The CAISON format's canonical types, parser, and printer (formerly
//! "CSON") have moved to the standalone [`caison`] crate
//! (<https://github.com/nixpt/caison>), which this crate re-exports in
//! full so downstream code that used to reach them through `crush_cson::*`
//! keeps compiling. `crush-cson` itself now carries only the Crush-specific
//! VM host capability ([`vm_cap::CsonParseCap`], registered as `cson.parse`
//! — the capability name is intentionally NOT renamed here; that is a
//! separate Crush-language-API decision, tracked outside CRUSH-CAISON-1).
//!
//! `caison` diverged from this crate's original fork (spec fixes, printer
//! rewrite, metadata-order changes) — consult `caison`'s own docs, not this
//! crate's history, for the current format semantics.

pub mod vm_cap;

pub use caison::*;
