use crush_diagnostics::DiagRecord;
use serde::Serialize;

const DEFAULT_DEJAVUE_DIR: &str = ".dejavue";

/// The AI Linter Engine that augments standard compiler errors.
pub struct AiLinter {
    enabled: bool,
    /// Directory the linter reads `.dejavue/decisions.md` from and
    /// appends `<dir>/timeline.jsonl` to. Defaults to `.dejavue`
    /// (production). Override via [`AiLinter::with_dejavue_dir`] so tests
    /// can redirect to a scratch directory without polluting the
    /// workspace's real Dejavue memory.
    dejavue_dir: String,
}

/// Structure for writing Dejavue timeline entries from crush-lint.
///
/// Serializing this with serde_json handles all RFC 8259 escaping
/// internally — no manual `replace("\"", "\\\"")` dance at the call site.
#[derive(Serialize)]
struct DejavueTimelineEntry<'a> {
    ts: u64,
    agent: &'a str,
    event: &'a str,
    path: &'a str,
    summary: &'a str,
    hint: &'a str,
}

impl AiLinter {
    /// Construct a linter pointing at the production Dejavue directory
    /// (`.dejavue/`). The hint generator is heuristic (diagnostic + Dejavue
    /// memory + source line); there is no model inference — the ONNX
    /// session stub and its `ort` dependency were removed (CRUSH-256), as no
    /// model was ever shipped.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            dejavue_dir: DEFAULT_DEJAVUE_DIR.to_string(),
        }
    }

    /// Builder-method: override the Dejavue directory (for tests that
    /// need to redirect the read/append paths to a scratch dir without
    /// polluting the workspace). Production callers use the default
    /// (`.dejavue/`).
    ///
    /// Strips a single trailing `/` if present so path-concatenation
    /// helpers (`format!("{}/decisions.md", dir)`) don't produce
    /// double-slashes when callers pass a directory-end-with-slash.
    pub fn with_dejavue_dir(mut self, dir: impl Into<String>) -> Self {
        let s = dir.into();
        // Strip a trailing `/` so concatenation helpers don't produce
        // double-slashes when callers pass `format!("{dir}/")` form.
        // Refuse to fully strip to empty — `with_dejavue_dir("/")` would
        // otherwise default `dejavue_dir` to `""` and produce
        // `/decisions.md` paths that don't belong to a Dejavue dir.
        let stripped = s.strip_suffix('/').map(str::to_string).unwrap_or(s);
        self.dejavue_dir = if stripped.is_empty() {
            DEFAULT_DEJAVUE_DIR.to_string()
        } else {
            stripped
        };
        self
    }

    /// Augment a standard diagnostic record with an AI-generated hint.
    ///
    /// When the linter is disabled (`enabled = false` at construction),
    /// returns the diagnostic unchanged. Otherwise, synthesizes a
    /// contextual hint from the diagnostic message + project memory
    /// (Dejavue `decisions.md` snippet, if present) + source line, and
    /// appends a `lint_error` event to `.dejavue/timeline.jsonl` (if the
    /// Dejavue directory exists) so the next agent's memory trace sees
    /// what just failed.
    ///
    /// The hint is printed to stderr (not returned on the record) because
    /// [`DiagRecord<'a>`] borrows from the caller — synthesized `String`
    /// hints cannot outlive this function without an arena. Callers that own
    /// their strings (the `crush-lint` binary) use [`AiLinter::hint_for`]
    /// and [`AiLinter::log_to_timeline`] and put the hint on the record.
    pub fn augment_diagnostic<'a>(
        &self,
        diag: DiagRecord<'a>,
        source_context: &str,
    ) -> DiagRecord<'a> {
        if !self.enabled {
            return diag;
        }

        let Some(hint) = self.hint_for(diag.message, source_context) else {
            // No applicable hint: say nothing rather than invent advice.
            let file_path = diag.file.unwrap_or("unknown");
            self.append_dejavue_timeline(file_path, diag.message, "");
            return diag;
        };
        eprintln!("\n[crush-lint] {}", hint);

        // --- Dejavue Context Protocol (DCP) Write Integration ---
        // We log compiler/linter errors directly to the project memory!
        // This way, the next AI Agent immediately sees what failed in its memory trace.
        let file_path = diag.file.unwrap_or("unknown");
        self.append_dejavue_timeline(file_path, diag.message, &hint);

        diag
    }

    /// Suggest a fix for a diagnostic, or `None` when there is nothing
    /// useful to say. Hints are advice attached to a diagnostic (the
    /// `hint` field), never a diagnostic of their own: they cannot raise
    /// an error and only ever elaborate on a message the real parser or
    /// semantic checker already produced. (Until CRUSH-316 a "Missing
    /// semicolon" arm here turned valid Crush into an `error`; Crush has
    /// no semicolons.)
    pub fn hint_for(&self, message: &str, source_context: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }
        if let Some(name) = message.strip_prefix("Undefined variable: ") {
            Some(format!(
                "declare `{name}` with `let {name} = ...` before using it, or check its spelling."
            ))
        } else if message.starts_with("Unexpected token") || message.starts_with("Expected ") {
            Some(format!(
                "check for unmatched brackets or a misspelled keyword near '{}'.",
                source_context.trim()
            ))
        } else {
            None
        }
    }

    /// Append a `lint_error` event to the Dejavue timeline (best effort;
    /// see [`AiLinter::augment_diagnostic`]).
    pub fn log_to_timeline(&self, source_path: &str, summary: &str, hint: &str) {
        if self.enabled {
            self.append_dejavue_timeline(source_path, summary, hint);
        }
    }

    /// Append a `lint_error` event to `<dejavue_dir>/timeline.jsonl` via
    /// serde_json (handles RFC 8259 escaping; no manual `replace`).
    /// Silently no-ops when the Dejavue directory is absent or the
    /// write fails — this is best-effort memory logging, not a gate.
    ///
    /// The directory comes from `self.dejavue_dir` so tests can redirect
    /// to a tempdir without touching the workspace's real Dejavue memory.
    fn append_dejavue_timeline(&self, source_path: &str, summary: &str, hint: &str) {
        let timeline = format!("{}/timeline.jsonl", self.dejavue_dir);
        if !std::path::Path::new(&self.dejavue_dir).exists() {
            return;
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = DejavueTimelineEntry {
            ts,
            agent: "crush-lint",
            event: "lint_error",
            path: source_path,
            summary,
            hint,
        };
        let Ok(json) = serde_json::to_string(&entry) else {
            return;
        };
        use std::io::Write;
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&timeline)
        else {
            return;
        };
        writeln!(file, "{}", json).ok();
    }
}

/// One finding from [`lint_source`], with owned strings so the caller can
/// render it as human text or as a `crush_diagnostics::DiagRecord`.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Canonical code: `E-SYNTAX` (parser), `E-SEMA` (semantic check), or
    /// the code a `crush-frontend` analysis pass assigns (`E-EXH-001`, ...).
    pub code: String,
    /// `"error"` or `"warning"`.
    pub level: &'static str,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub message: String,
}

fn parse_error_pos(e: &crush_frontend::parser::ParseError) -> (Option<u32>, Option<u32>) {
    use crush_frontend::parser::ParseError as P;
    let (l, c) = match e {
        P::UnexpectedToken { line, col, .. }
        | P::Expected { line, col, .. }
        | P::UnexpectedEOF { line, col }
        | P::InvalidNumber { line, col, .. }
        | P::UnterminatedString { line, col }
        | P::UnknownAnnotation { line, col, .. } => (*line, *col),
    };
    (Some(l as u32), Some(c as u32))
}

/// Lint Crush source with the real front end: the parser (syntax errors),
/// the compiler's analysis passes (`check_source`), and the semantic checker
/// (undefined variables, type errors). Nothing here guesses at syntax: a
/// program `crush-frontend` accepts gets no `E-SYNTAX` finding.
///
/// The semantic checker stops at its first error and carries no source
/// position, so an `E-SEMA` finding has `line: None`.
pub fn lint_source(source: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Err(errors) = crush_frontend::parser::Parser::parse(source) {
        for e in &errors {
            let (line, col) = parse_error_pos(e);
            out.push(Finding {
                code: "E-SYNTAX".into(),
                level: "error",
                line,
                col,
                message: e.to_string(),
            });
        }
        return out;
    }
    // Parsed once above for structured positions, again here so the pass
    // list stays defined in one place (`check_source`).
    let (program, diags) = match crush_frontend::check_source(source) {
        Ok(v) => v,
        Err(e) => {
            out.push(Finding {
                code: "E-SYNTAX".into(),
                level: "error",
                line: None,
                col: None,
                message: format!("{e:#}"),
            });
            return out;
        }
    };
    for d in diags {
        let level = match d.severity {
            crush_frontend::diagnostics::DiagnosticSeverity::Error => "error",
            crush_frontend::diagnostics::DiagnosticSeverity::Warning => "warning",
        };
        let line = (d.location.line > 0).then_some(d.location.line);
        let col = (d.location.col > 0).then_some(d.location.col);
        let mut message = d.message;
        if let Some(h) = d.hint {
            message = format!("{message} ({h})");
        }
        out.push(Finding {
            code: d.code.to_string(),
            level,
            line,
            col,
            message,
        });
    }
    if let Err(e) = crush_frontend::lower_and_check(program) {
        out.push(Finding {
            code: "E-SEMA".into(),
            level: "error",
            line: None,
            col: None,
            message: format!("{e:#}"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction & disabled-linter passthrough ─────────────────────────

    #[test]
    fn disabled_linter_is_disabled() {
        let linter = AiLinter::new(false);
        assert!(!linter.enabled);
    }

    #[test]
    fn enabled_linter_constructs() {
        let linter = AiLinter::new(true);
        assert!(linter.enabled);
    }

    #[test]
    fn disabled_linter_returns_diag_unchanged() {
        let linter = AiLinter::new(false);
        let diag = DiagRecord {
            code: "E-TEST",
            level: "error",
            file: Some("test.crush"),
            line: Some(42),
            col: None,
            message: "test message",
            hint: None,
        };
        // DiagRecord doesn't derive Clone, so compare fields directly.
        let out = linter.augment_diagnostic(diag, "src context");
        assert_eq!(out.code, "E-TEST");
        assert_eq!(out.message, "test message");
        assert_eq!(out.hint, None);
        assert_eq!(out.file, Some("test.crush"));
        assert_eq!(out.line, Some(42));
        assert_eq!(out.col, None);
    }

    #[test]
    fn with_dejavue_dir_strips_trailing_slash() {
        // Path with trailing `/` must produce a path WITHOUT a double
        // slash when format-concatenated with another `/`.
        let linter = AiLinter::new(false).with_dejavue_dir("/tmp/foo/");
        assert!(!linter.dejavue_dir.ends_with('/'));
        // Verify concat doesn't produce `//`.
        let path = format!("{}/decisions.md", linter.dejavue_dir);
        assert!(!path.contains("//"), "double-slash in concat: {path}");
    }

    #[test]
    fn with_dejavue_dir_root_slash_falls_back_to_default() {
        // Passing "/" would strip to empty string; without a guard, the
        // linter would have `dejavue_dir = ""` and produce `/decisions.md`
        // paths that don't belong to any Dejavue dir. The guard catches
        // this and falls back to the production default.
        let linter = AiLinter::new(false).with_dejavue_dir("/");
        assert_eq!(linter.dejavue_dir, DEFAULT_DEJAVUE_DIR);
    }

    #[test]
    fn with_dejavue_dir_empty_string_falls_back_to_default() {
        // Same guard fires for the explicit empty-string case. Goes
        // through a different branch (`strip_suffix('/')` returns None on
        // an empty input, then `is_empty()` catches it). Split out from
        // the root-slash test for cleaner failure isolation.
        let linter = AiLinter::new(false).with_dejavue_dir("");
        assert_eq!(linter.dejavue_dir, DEFAULT_DEJAVUE_DIR);
    }

    // ── Enabled linter: end-to-end "doesn't panic" ──────────────────────────
    //
    // Redirection to a scratch Dejavue directory (via `with_dejavue_dir`)
    // keeps the workspace's real `.dejavue/timeline.jsonl` untouched.
    // The directory name includes PID + nanosecond timestamp for
    // uniqueness across parallel test runs.
    #[test]
    fn enabled_linter_augment_diagnostic_does_not_panic() {
        let scratch = std::env::temp_dir().join(format!(
            "crush-lint-augment-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(&scratch).unwrap();

        let linter = AiLinter::new(true).with_dejavue_dir(scratch.to_string_lossy());
        let diag = DiagRecord {
            code: "E-MISSING-SEMI",
            level: "error",
            file: Some("src/example.crush"),
            line: Some(7),
            col: None,
            message: "Missing semicolon on line 7",
            hint: None,
        };
        // Must not panic regardless of workspace state. The augmentation
        // path prints to stderr (acceptable in tests) and appends to
        // `<scratch>/timeline.jsonl` (verified below).
        linter.augment_diagnostic(diag, "let x = 5");

        // Verify the timeline append actually happened — proves the
        // Dejavue write path operates end-to-end without pollution.
        let timeline = std::fs::read_to_string(scratch.join("timeline.jsonl"))
            .expect("scratch timeline.jsonl must exist");
        assert!(
            timeline.contains("\"event\":\"lint_error\""),
            "missing lint_error event in: {timeline}"
        );
        assert!(
            timeline.contains("\"path\":\"src/example.crush\""),
            "missing source path in: {timeline}"
        );
        assert!(
            timeline.contains("Missing semicolon"),
            "missing summary in: {timeline}"
        );

        std::fs::remove_dir_all(&scratch).ok();
    }

    // The `with_dejavue_dir` override is exercised end-to-end by
    // `enabled_linter_augment_diagnostic_does_not_panic` above — that
    // test redirects to a scratch dir, runs augmentation, and asserts the
    // timeline.jsonl got the expected entries. No separate redirect test
    // is needed; the tautological `X || !X` assertion that early draft
    // used has been removed.

    // ── JSON serialization ──────────────────────────────────────────────────

    #[test]
    fn dejavue_entry_json_handles_special_characters() {
        // Ensures RFC 8259 escaping via serde_json handles quotes,
        // newlines, and backslashes in the hint field — the failure mode
        // for the old manual `replace("\"", "\\\"")` path.
        let entry = DejavueTimelineEntry {
            ts: 1234567890,
            agent: "crush-lint",
            event: "lint_error",
            path: "src/x.rs",
            summary: "issue with \"quotes\" and \\backslashes\\",
            hint: "line1\nline2\ttabbed",
        };
        let json = serde_json::to_string(&entry).expect("serialization should not fail");
        assert!(json.contains("\"hint\":\"line1\\nline2\\ttabbed\""));
        assert!(json.contains("\\\"quotes\\\""));
        assert!(json.contains("\\\\backslashes\\\\"));
        // And it must round-trip back into an equivalent value.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["summary"],
            "issue with \"quotes\" and \\backslashes\\"
        );
        assert_eq!(parsed["hint"], "line1\nline2\ttabbed");
    }
}
