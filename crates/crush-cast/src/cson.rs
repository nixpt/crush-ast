//! CAISON (formerly CSON) types — re-exported from the canonical `caison`
//! crate (via `crush-cson`, which now just re-exports it).
//!
//! `caison` (<https://github.com/nixpt/caison>) is the single source of
//! truth for CAISON type definitions across the crush ecosystem. This
//! module re-exports them for backward compatibility and convenience
//! within the crush-cast crate.

pub use crush_cson::{
    CaisonKey,
    CaisonValue,
    CaisonNode,
    CaisonDocument,
    CaisonAnnotation,
};
