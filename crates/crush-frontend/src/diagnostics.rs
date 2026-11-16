//! Compiler diagnostic types for crush-frontend.
//!
//! Diagnostics are produced by analysis passes (e.g. `exhaustive_check`) and
//! returned alongside the enriched CAST from `check_source()`.  They are
//! distinct from parse errors (which short-circuit with `Err`) — diagnostics
//! are non-fatal warnings and hints the caller may surface to the author.

use crush_cast::manifest::SourceLoc;

/// Severity level of a compiler diagnostic.
///
/// `#[non_exhaustive]`: more levels may be added. A consumer should treat a
/// level it does not know as the least severe one (like `Note`: shown, never
/// failing a build).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    /// Information only: never fails a build, not even under
    /// `--deny-warnings` (the legacy all-public module note, `N-VIS01`).
    Note,
    Warning,
    Error,
}

impl DiagnosticSeverity {
    /// The NDJSON `level` string (`"note"`, `"warning"`, `"error"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticSeverity::Note => "note",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Error => "error",
        }
    }
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A diagnostic emitted by a compiler analysis pass.
///
/// Returned as part of `check_source()` — the caller decides whether to
/// surface as a build warning, log entry, or CI failure depending on severity.
#[derive(Debug, Clone)]
pub struct CompilerDiagnostic {
    /// Machine-stable code.  Prefix `E-` for errors, `W-` for warnings,
    /// `N-` for notes.
    pub code: &'static str,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Human-readable message.
    pub message: String,
    /// Source location of the offending node.
    pub location: SourceLoc,
    /// Optional one-liner hint for how to resolve the issue.
    pub hint: Option<String>,
}

impl std::fmt::Display for CompilerDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}:{}: {}: {}",
            self.code, self.location.file, self.location.line, self.severity, self.message
        )?;
        if let Some(h) = &self.hint {
            write!(f, "\n  hint: {h}")?;
        }
        Ok(())
    }
}
