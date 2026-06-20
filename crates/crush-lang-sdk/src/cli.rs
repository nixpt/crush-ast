//! Shared CLI helpers for the `crushc` / `crush-run` / `crush-compile`
//! binaries.
//!
//! This module currently hosts [`MessageFormat`], the `--message-format`
//! `<FORMAT>` parser used by all three binaries. Extracted as the third
//! per-binary copy landed — at three call-sites the duplication cost is
//! visible and the surface (one enum + one `FromStr` impl) is comfortably
//! small. Future CLI-shared helpers (e.g. `--cap` arg parsing) can land
//! here without repeating the extraction round-trip.

/// Diagnostic output mode for CLI binaries on error.
///
/// `Text` is the default and preserves each binary's historical human-
/// readable prefix (`crushc:`, `crush-run: …`, `crush-compile:`).
/// `Json` emits one NDJSON record per error to stderr for editor / IDE /
/// LSP bridge integration (see [`crate::theme::JsonDiagnostic`]).
#[derive(Clone, Copy, PartialEq)]
pub enum MessageFormat {
    /// Human-readable terminal output (default).
    Text,
    /// Single NDJSON record on stderr for tool consumption.
    Json,
}

impl std::str::FromStr for MessageFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(MessageFormat::Text),
            "json" => Ok(MessageFormat::Json),
            _ => Err(format!(
                "unknown message format '{s}' (expected: text, json)"
            )),
        }
    }
}
