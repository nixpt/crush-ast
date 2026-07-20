use crush_diagnostics::DiagRecord;
use ort::session::Session;
use serde::Serialize;
use std::sync::Arc;
use parking_lot::Mutex;

pub const LINT_MODEL_PATH: &str = "crates/crush-lint/models/lint_model.onnx";
const DEFAULT_DEJAVUE_DIR: &str = ".dejavue";

/// The AI Linter Engine that augments standard compiler errors.
pub struct AiLinter {
    /// Stub field for future embedding inference. Currently always `None`
    /// because `try_load_session` only succeeds when the bundled ONNX
    /// model is present (typically not in CI/test environments). Kept
    /// on the struct so the public API contract ("model-backed linter")
    /// is documented and stable while the embedding path is still
    /// scaffolded. Will be consumed by `augment_diagnostic` once the
    /// inference loop lands.
    #[allow(dead_code)]
    session: Option<Arc<Mutex<Session>>>,
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
    /// (`.dejavue/`). When `enabled` is true, attempts to load the
    /// bundled ONNX model from [`LINT_MODEL_PATH`]. If the model file
    /// is missing or fails to load (corrupt, wrong format, ORT builder
    /// error), the linter silently falls back to the enabled-but-no-
    /// session path — the heuristic hint generator still runs without
    /// embeddings. This avoids `.unwrap()` panics in production builds
    /// where the model may not yet exist or may be a developer-machine-
    /// specific file checked out of sync.
    pub fn new(enabled: bool) -> Self {
        let session = if enabled {
            Self::try_load_session(LINT_MODEL_PATH).ok()
        } else {
            None
        };
        Self {
            session,
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

    /// Attempt to build and load the ONNX session. Returns `Err` on any
    /// failure (builder init, model file missing, `commit_from_file`
    /// rejection). Callers should downgrade gracefully (`.ok()` and
    /// continue without embeddings) rather than panic — a broken model
    /// is an environment issue, not a request error.
    ///
    /// Returns [`std::io::Error`] so callers can `.ok()` without
    /// depending on ORT's private error API; the ORT error is wrapped
    /// in a stage-labelled message (`"ORT builder: ..."` /
    /// `"ORT commit_from_file: ..."`) for debugability.
    ///
    /// Mirrors the proven pattern from `crush-vm/src/ai_optimizer`:
    /// `Session::builder()?.commit_from_file(path)?` — both methods take
    /// `&self` and return owned results, so the chain is straight-line
    /// `?`-propagation on `Result<_, std::io::Error>` (lifted via a
    /// single inline `map_err`) without any mutable-borrow dance.
    fn try_load_session(path: &str) -> Result<Arc<Mutex<Session>>, std::io::Error> {
        use std::io::Error;
        use std::path::Path;
        if !Path::new(path).exists() {
            return Err(Error::new(
                std::io::ErrorKind::NotFound,
                format!("linter model file not found: {path}"),
            ));
        }
        let sess = ort::session::Session::builder()
            .map_err(|e| Error::other(format!("ORT builder: {e}")))?
            .commit_from_file(path)
            .map_err(|e| {
                Error::other(format!("ORT commit_from_file({path}): {e}"))
            })?;
        Ok(Arc::new(Mutex::new(sess)))
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
    /// hints cannot outlive this function without an arena. Future
    /// work: integrate embeddings via `self.session` (currently a stub).
    pub fn augment_diagnostic<'a>(&self, diag: DiagRecord<'a>, source_context: &str) -> DiagRecord<'a> {
        if !self.enabled {
            return diag;
        }

        // In a real implementation, we would convert `source_context` to an embedding
        // vector and run it through `self.session`.

        // --- Dejavue Context Protocol (DCP) Integration ---
        // An AI-Native linter should be aware of project memory.
        let dejavue_context = self.load_dejavue_context();

        // --- Simulated AI Inference ---
        // All three arms always produce a hint, so the `Option<String>`
        // wrapper would be pointless ceremony — use a plain `String`.
        let hint = if diag.message.contains("Missing semicolon") {
            format!("Hint: It looks like you forgot a semicolon after '{}'.{}", source_context.trim(), dejavue_context)
        } else if diag.message.contains("Unexpected token") {
            format!("Hint: Check for unmatched brackets near '{}'.{}", source_context.trim(), dejavue_context)
        } else {
            // General embedding-based similarity hint
            format!("Hint: Based on typical patterns, you might need to refactor '{}'.{}", source_context.trim(), dejavue_context)
        };
        // ------------------------------

        // ---
        // For the sake of this SDK demo, we just print the AI hint to stderr,
        // because DiagRecord only accepts `&'a str`.
        eprintln!("\n🤖 [AI Linter] {}", hint);

        // --- Dejavue Context Protocol (DCP) Write Integration ---
        // We log compiler/linter errors directly to the project memory!
        // This way, the next AI Agent immediately sees what failed in its memory trace.
        let file_path = diag.file.unwrap_or("unknown");
        self.append_dejavue_timeline(file_path, diag.message, &hint);

        diag
    }

    /// Load the first 100 chars of `<dejavue_dir>/decisions.md` as a
    /// summary snippet to inject into AI hints. Empty string when the
    /// file is absent or unreadable — callers append without conditionals.
    ///
    /// The directory comes from `self.dejavue_dir` (default `.dejavue`,
    /// overridable for tests via `with_dejavue_dir`).
    fn load_dejavue_context(&self) -> String {
        let path = format!("{}/decisions.md", self.dejavue_dir);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return String::new();
        };
        let mut s = String::from(" Project Context from .dejavue: ");
        s.push_str(&content.chars().take(100).collect::<String>());
        s.push_str("...");
        s
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
        let Ok(json) = serde_json::to_string(&entry) else { return };
        use std::io::Write;
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true).append(true).open(&timeline)
        else { return };
        writeln!(file, "{}", json).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction & disabled-linter passthrough ─────────────────────────

    #[test]
    fn disabled_linter_has_no_session() {
        let linter = AiLinter::new(false);
        assert!(!linter.enabled);
        assert!(linter.session.is_none());
    }

    #[test]
    fn enabled_linter_without_model_falls_back_gracefully() {
        // Model file at LINT_MODEL_PATH does not exist in test environment
        // (the .onnx file is not checked in). The linter must not panic
        // — it should silently fall back to enabled-but-no-session.
        let linter = AiLinter::new(true);
        assert!(linter.enabled);
        assert!(linter.session.is_none(), "should fall back when model file is absent");
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

    // ── Dejavue context loading via with_dejavue_dir redirect ─────────────────
    //
    // TheContext loader reads `<self.dejavue_dir>/decisions.md`. Tests use
    // `with_dejavue_dir(scratch)` to redirect to a controlled dir so the
    // workspace's real `.dejavue/decisions.md` doesn't contaminate the
    // assertions.
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "crush-lint-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ))
    }

    #[test]
    fn load_dejavue_context_returns_empty_when_dir_absent() {
        // Scratch dir simply doesn't exist (we never create it).
        let scratch = scratch_dir("ctx-empty");
        let linter = AiLinter::new(false).with_dejavue_dir(scratch.to_string_lossy());
        let ctx = linter.load_dejavue_context();
        assert!(ctx.is_empty(), "got unexpected content: {ctx:?}");
    }

    #[test]
    fn load_dejavue_context_populates_when_decisions_md_exists() {
        let scratch = scratch_dir("ctx-populated");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(
            scratch.join("decisions.md"),
            "// Custom context line for the test",
        )
        .unwrap();
        let linter = AiLinter::new(false).with_dejavue_dir(scratch.to_string_lossy());
        let ctx = linter.load_dejavue_context();
        assert!(ctx.contains("Project Context from .dejavue"));
        assert!(ctx.contains("Custom context line"));
        std::fs::remove_dir_all(&scratch).ok();
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

    // ── Enabled-but-no-session linter: end-to-end "doesn't panic" ────────────
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
        assert!(linter.session.is_none());
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
        assert!(timeline.contains("\"event\":\"lint_error\""), "missing lint_error event in: {timeline}");
        assert!(timeline.contains("\"path\":\"src/example.crush\""), "missing source path in: {timeline}");
        assert!(timeline.contains("Missing semicolon"), "missing summary in: {timeline}");

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
        assert_eq!(parsed["summary"], "issue with \"quotes\" and \\backslashes\\");
        assert_eq!(parsed["hint"], "line1\nline2\ttabbed");
    }
}
