//! Unified version-mismatch surface (VER-05 — the v1.7 convergence anchor).
//!
//! Exosphere enforces a version check at four load-time boundaries:
//!
//! | boundary    | gate site                              | req    |
//! |-------------|----------------------------------------|--------|
//! | `ipc`       | `wave3-kernel::abi` (`Envelope`)       | (v1.5) |
//! | `manifest`  | capsule manifest loader                | VER-01 |
//! | `casm`      | `casm::Program::deserialize`           | VER-02 |
//! | `cap_schema`| `cap-engine` security-policy registry  | VER-03 |
//! | `service`   | `exo-service` endpoint handshake       | VER-04 |
//! | `cast`      | `crush_cast::Program::deserialize`     | EXO-176|
//!
//! Before VER-05 each gate would invent its own error shape, so the control-pane
//! and audit log could not render version failures uniformly. This module is the
//! single shape every gate maps onto. Each boundary keeps its own typed gate
//! error (e.g. `kernel::abi::IpcError::VersionMismatch`, `convert::casm::CasmError::Version`)
//! and produces a [`VersionMismatch`] for reporting — the gate ergonomics stay
//! local, the reporting surface is unified.
//!
//! `expected`/`found` are kept as `String` so heterogeneous version encodings
//! (the `u8` ABI version, semver strings) share one type. [`VersionMismatch`] is
//! `Serialize`, so it doubles as the audit payload — the `boundary` discriminant
//! travels with every logged failure.

use serde::{Deserialize, Serialize};

use crate::{CrushError, ErrorKind};

/// Which of the four load-time version boundaries produced a mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionBoundary {
    /// IPC/ABI envelope wire version (`wave3-kernel::abi`, enforced since v1.5).
    Ipc,
    /// Capsule manifest version (VER-01).
    Manifest,
    /// CASM bytecode format version (VER-02).
    Casm,
    /// Capability-schema version (VER-03).
    CapSchema,
    /// Service-API version (VER-04).
    Service,
    /// CAST IR pack-format version (EXO-176).
    Cast,
}

impl VersionBoundary {
    /// Stable lowercase discriminant — the value carried in audit records.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ipc => "ipc",
            Self::Manifest => "manifest",
            Self::Casm => "casm",
            Self::CapSchema => "cap_schema",
            Self::Service => "service",
            Self::Cast => "cast",
        }
    }
}

impl std::fmt::Display for VersionBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A version incompatibility detected at a load-time boundary — the unified
/// shape (VER-05) behind every gate.
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

    /// IPC/ABI envelope boundary — maps `wave3-kernel::abi::IpcError::VersionMismatch`
    /// (whose `u8` versions are stringified here) onto the unified shape without
    /// disturbing the ABI error itself.
    pub fn ipc(expected: u8, found: u8) -> Self {
        Self::new(
            VersionBoundary::Ipc,
            expected.to_string(),
            found.to_string(),
        )
    }

    /// Capsule-manifest boundary (VER-01).
    pub fn manifest(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::new(VersionBoundary::Manifest, expected, found)
    }

    /// CASM-bytecode boundary (VER-02).
    pub fn casm(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::new(VersionBoundary::Casm, expected, found)
    }

    /// Capability-schema boundary (VER-03).
    pub fn cap_schema(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::new(VersionBoundary::CapSchema, expected, found)
    }

    /// Service-API boundary (VER-04).
    pub fn service(expected: impl Into<String>, found: impl Into<String>) -> Self {
        Self::new(VersionBoundary::Service, expected, found)
    }

    /// CAST IR pack-format boundary (EXO-176).
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
    fn ipc_boundary_stringifies_u8_versions() {
        let v = VersionMismatch::ipc(1, 2);
        assert_eq!(v.boundary, VersionBoundary::Ipc);
        assert_eq!(v.expected, "1");
        assert_eq!(v.found, "2");
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
        let v = VersionMismatch::service("v1", "v2");
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"boundary\":\"service\""), "{json}");
        assert!(json.contains("\"expected\":\"v1\""), "{json}");
    }

    #[test]
    fn every_boundary_has_a_stable_discriminant() {
        for (b, s) in [
            (VersionBoundary::Ipc, "ipc"),
            (VersionBoundary::Manifest, "manifest"),
            (VersionBoundary::Casm, "casm"),
            (VersionBoundary::CapSchema, "cap_schema"),
            (VersionBoundary::Service, "service"),
        ] {
            assert_eq!(b.as_str(), s);
        }
    }

    #[test]
    fn converts_to_crush_error_as_invalid_argument() {
        let err: CrushError = VersionMismatch::manifest("1", "9").into();
        assert!(err.to_string().contains("version mismatch"));
    }
}
