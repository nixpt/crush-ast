//! Machine-readable pack format for CAST programs (EXO-176).
//!
//! CAST is dual-format, mirroring the CASM `Format` story one layer below:
//!
//! - **JSON** (`.cast.json`) — the canonical authoring/debug form. Pretty-printed,
//!   language-neutral, what `examples/cast/` and the cookbook are written in.
//! - **Binary** (`.castb`) — CBOR via `cbor4ii` (the one binary codec family
//!   across the stack — the mesh wire codec already uses it). The encoding is
//!   exactly the serde derivation of the existing types; there is no custom
//!   header or magic. Format selection on load is by file extension
//!   (`.castb` or `.cbor` ⇒ Binary, anything else ⇒ JSON) via
//!   [`Format::from_path`], with the explicit [`Format`] argument API
//!   underneath for callers that know what they hold.
//!
//! Both load paths fail closed on an incompatible `cast_version`:
//! [`Program::deserialize`] always runs [`Program::check_version`], which gates
//! on the **major** component against [`CAST_VERSION`] — modeled on the CASM
//! VER-02 gate, reporting through the unified `crush_errors::VersionMismatch`
//! shape (boundary = `cast`).

use std::path::Path;

use crate::Program;

/// The CAST IR format version this crate supports.
///
/// Compatibility is gated on the **major** component: a program whose
/// `cast_version` major differs (or is unparseable) is rejected at load time
/// with [`PackError::Version`]. Minor bumps stay compatible — the existing
/// corpus carries both `"0.1"` and `"0.1.0"`.
pub const CAST_VERSION: &str = "0.1";

/// Serialization format for CAST programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable canonical form (`.cast.json`).
    Json,
    /// Compact CBOR binary (`.castb`).
    Binary,
}

impl Format {
    /// Detect format from file extension: `.castb` (the convention) or `.cbor`
    /// map to [`Format::Binary`]; anything else is treated as JSON.
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("castb") | Some("cbor") => Format::Binary,
            _ => Format::Json,
        }
    }
}

/// Errors from the pack/unpack surface.
#[derive(Debug)]
pub enum PackError {
    /// Encoding failed (either codec).
    Serialization(String),
    /// Decoding failed (either codec).
    Deserialization(String),
    /// `cast_version` major is incompatible with [`CAST_VERSION`].
    Version(crush_errors::VersionMismatch),
    /// File read/write failed (only from [`Program::save`] / [`Program::load`]).
    Io(String),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialization(e) => write!(f, "CAST serialization failed: {}", e),
            Self::Deserialization(e) => write!(f, "CAST deserialization failed: {}", e),
            Self::Version(v) => write!(f, "{}", v),
            Self::Io(e) => write!(f, "CAST file I/O failed: {}", e),
        }
    }
}

impl std::error::Error for PackError {}

impl Program {
    /// Serialize the program to bytes in the given format.
    ///
    /// JSON is pretty-printed (it is the debug/authoring form); Binary is
    /// plain CBOR with no header.
    pub fn serialize(&self, format: Format) -> Result<Vec<u8>, PackError> {
        match format {
            Format::Json => {
                serde_json::to_vec_pretty(self).map_err(|e| PackError::Serialization(e.to_string()))
            }
            Format::Binary => cbor4ii::serde::to_vec(Vec::new(), self)
                .map_err(|e| PackError::Serialization(e.to_string())),
        }
    }

    /// Major version component, if `version` parses as `MAJOR[.MINOR...]`.
    fn major_of(version: &str) -> Option<u32> {
        version.split('.').next()?.parse().ok()
    }

    /// Load-time CAST version gate (EXO-176).
    ///
    /// Accepts programs whose `cast_version` major matches [`CAST_VERSION`]'s
    /// major; rejects a differing major or an unparseable version with
    /// [`PackError::Version`] carrying the unified
    /// `crush_errors::VersionMismatch` (boundary = `cast`). Modeled on the
    /// CASM VER-02 gate.
    pub fn check_version(&self) -> Result<(), PackError> {
        let supported_major = Self::major_of(CAST_VERSION);
        match Self::major_of(&self.cast_version) {
            Some(found) if Some(found) == supported_major => Ok(()),
            _ => Err(PackError::Version(crush_errors::VersionMismatch::cast(
                CAST_VERSION,
                self.cast_version.clone(),
            ))),
        }
    }

    /// Deserialize a program from bytes.
    ///
    /// Enforces the [`check_version`](Self::check_version) gate after parsing
    /// so every load path (JSON or Binary) fails closed on an incompatible
    /// `cast_version`.
    pub fn deserialize(data: &[u8], format: Format) -> Result<Self, PackError> {
        let program: Self = match format {
            Format::Json => serde_json::from_slice(data)
                .map_err(|e| PackError::Deserialization(e.to_string()))?,
            Format::Binary => cbor4ii::serde::from_slice(data)
                .map_err(|e| PackError::Deserialization(e.to_string()))?,
        };
        program.check_version()?;
        Ok(program)
    }

    /// Save the program to a file (format detected from extension).
    pub fn save(&self, path: &Path) -> Result<(), PackError> {
        let data = self.serialize(Format::from_path(path))?;
        std::fs::write(path, data).map_err(|e| PackError::Io(e.to_string()))
    }

    /// Load a program from a file (format detected from extension).
    ///
    /// Runs the version gate on every load, same as [`Self::deserialize`].
    pub fn load(path: &Path) -> Result<Self, PackError> {
        let data = std::fs::read(path).map_err(|e| PackError::Io(e.to_string()))?;
        Self::deserialize(&data, Format::from_path(path))
    }
}
