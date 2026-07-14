//! CSON types — re-exported from the canonical `crush-cson` crate.
//!
//! `crush-cson` is the single source of truth for CSON type definitions
//! across the crush ecosystem. This module re-exports them for backward
//! compatibility and convenience within the crush-cast crate.

pub use crush_cson::{
    CsonKey,
    CsonValue,
    CsonNode,
    CsonDocument,
    CsonAnnotation,
};
