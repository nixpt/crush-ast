//! Compiler diagnostic types for crush-frontend.
//!
//! Diagnostics are produced by analysis passes (e.g. `exhaustive_check`) and
//! returned alongside the enriched CAST from `check_source()`.  They are
//! distinct from parse errors (which short-circuit with `Err`) — diagnostics
//! are non-fatal warnings and hints the caller may surface to the author.

use crush_cast::manifest::SourceLoc;

/// Severity level of a compiler diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticSeverity::Warning => write!(f, "warning"),
            DiagnosticSeverity::Error => write!(f, "error"),
        }
    }
}

/// A diagnostic emitted by a compiler analysis pass.
///
/// Returned as part of `check_source()` — the caller decides whether to
/// surface as a build warning, log entry, or CI failure depending on severity.
#[derive(Debug, Clone)]
pub struct CompilerDiagnostic {
    /// Machine-stable code.  Prefix `E-` for errors, `W-` for warnings.
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
