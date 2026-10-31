//! Unified version-mismatch surface.
//!
//! crush-ast checks a format version when it loads two kinds of artifact:
//!
//! | boundary | gate site                                         |
//! |----------|---------------------------------------------------|
//! | `casm`   | `casm::Program` deserialize (`CASM_VERSION` major) |
//! | `cast`   | `crush_cast::pack` unpack (CAST pack format)       |
//!
//! Each gate keeps its own typed error (`convert::casm::CasmError::Version`,
//! `crush_cast::pack::PackError::Version`) and produces a [`VersionMismatch`]
//! for reporting, so every version failure renders the same way.
//!
//! `expected`/`found` are `String`s so different version encodings share one
//! type. [`VersionMismatch`] is `Serialize`; the `boundary` discriminant travels
//! with it. [`VersionBoundary`] is `#[non_exhaustive]`: a new gate adds a variant.

use serde::{Deserialize, Serialize};

use crate::{CrushError, ErrorKind};

/// Which load-time version boundary produced a mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VersionBoundary {
    /// CASM bytecode format version.
    Casm,
    /// CAST pack-format version.
    Cast,
}

impl VersionBoundary {
    /// Stable lowercase discriminant — the value carried in audit records.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Casm => "casm",
            Self::Cast => "cast",
        }
    }
}

impl std::fmt::Display for VersionBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A version incompatibility detected at a load-time boundary — the one shape
/// every gate reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionMismatch {
    pub boundary: VersionBoundary,
    pub expected: String,
    pub found: String,
}

impl VersionMismatch {
    /// Construct a mismatch for an explicit boundary.
    pub fn new(
        boundary: VersionBoundary,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Self {
        Self {
            boundary,
            expected: expected.into(),
            found: found.into(),
        }
    }

    /// CASM-bytecode boundary.
    pub fn casm(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::new(VersionBoundary::Casm, expected, found)
    }

    /// CAST pack-format boundary.
    pub fn cast(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::new(VersionBoundary::Cast, expected, found)
    }
}

impl std::fmt::Display for VersionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "version mismatch at {} boundary: expected {}, found {}",
            self.boundary, self.expected, self.found
        )
    }
}

impl std::error::Error for VersionMismatch {}

impl From<VersionMismatch> for CrushError {
    fn from(v: VersionMismatch) -> Self {
        CrushError::new(ErrorKind::InvalidArgument, v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_the_boundary() {
        assert_eq!(VersionMismatch::casm("1.0", "2.0").boundary, VersionBoundary::Casm);
        let v = VersionMismatch::cast("1", "2");
        assert_eq!(v.boundary, VersionBoundary::Cast);
        assert_eq!((v.expected.as_str(), v.found.as_str()), ("1", "2"));
    }

    #[test]
    fn display_names_boundary_and_versions() {
        let v = VersionMismatch::casm("1.0", "2.0");
        let s = v.to_string();
        assert!(s.contains("casm"), "{s}");
        assert!(s.contains("1.0") && s.contains("2.0"), "{s}");
    }

    #[test]
    fn serializes_with_boundary_discriminant_for_audit() {
        // The audit log renders this directly — the boundary must be present.
        let v = VersionMismatch::cast("v1", "v2");
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"boundary\":\"cast\""), "{json}");
        assert!(json.contains("\"expected\":\"v1\""), "{json}");
    }

    #[test]
    fn every_boundary_has_a_stable_discriminant() {
        for (b, s) in [
            (VersionBoundary::Casm, "casm"),
            (VersionBoundary::Cast, "cast"),
        ] {
            assert_eq!(b.as_str(), s);
        }
    }

    #[test]
    fn converts_to_crush_error_as_invalid_argument() {
        let err: CrushError = VersionMismatch::casm("1", "9").into();
        assert!(err.to_string().contains("version mismatch"));
    }
}
