//! **Retired: `crush-cson` was renamed to [`crush-caison`](https://crates.io/crates/crush-caison).**
//!
//! CAISON ("Crush AI-native Semantic Object Notation") was called CSON until
//! 2026-09-27, when it was renamed because "CSON" already means CoffeeScript
//! Object Notation. This crate only re-exports `crush-caison` so existing
//! `crush_cson::…` paths keep compiling. Switch your dependency to
//! `crush-caison`; this crate gets no further releases.
//!
//! The VM capability is now `caison.parse`; `cson.parse` is a deprecated
//! alias removed in 0.5.

pub use crush_caison::*;
