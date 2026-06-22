//! Tests for the combined round-trip + cross-parser matrix domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1.
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};


