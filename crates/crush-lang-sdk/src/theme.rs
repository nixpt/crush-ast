//! Theme / pretty-printing layer for the Crush CLI surface.
//!
//! Single place where error and output formatting is styled for human readers.
//! Wraps the existing `Display` implementations of
//! [`crush_frontend::parser::ParseError`], [`crush_vm::VmError`] and our SDK's
//! [`RuntimeError`] with ANSI colors, severity badges, source snippets (with
//! line numbers and a caret pointer), and REPL prompt / result decoration.
//!
//! Auto-detection: `init_styling` is idempotent and inspects the `NO_COLOR`
//! convention plus whether stderr / stdout are attached to a terminal.
//! Either being false invokes `[yansi::disable]` so plain-text consumers (CI
//! logs, redirected files, agent harness captures) still get the structured
//! uncolored output instead of raw escape codes.
//!
//! Backwards compatibility: this module **never replaces** the underlying
//! `Display` text on existing error types. Tests and downstream tools that
//! grep substrings like `"instruction quota exceeded"` or
//! `"capability not declared"` keep working untouched.
//!
//! Public stable surface:
//! - [`Severity`] — discriminator for the leading `[…]` badge.
//! - [`init_styling`] — call once before printing anything.
//! - [`paint_error_badge`], [`paint_warning_badge`], [`paint_note_badge`],
//!   [`paint_path`], [`paint_dim`], [`paint_good`], [`paint_prompt`] —
//!   colored string helpers.
//! - [`render_source_snippet`] — pure formatter; no stderr side-effects.
//! - [`render_parse_error`], [`render_parse_errors`] — `ParseError` → string.
//! - [`render_runtime_error`] — anything `Display`-able → runtime badge +
//!   preserved text. Accepts `VmError`, `RuntimeError`, `anyhow::Error`.
//! - [`format_repl_prompt`], [`format_repl_result`] — REPL ergonomic shims.
//!
//! [`RuntimeError`]: crate::RuntimeError
//! [`yansi`]: https://crates.io/crates/yansi

use std::io::IsTerminal;
use std::sync::Once;

use crush_frontend::parser::ParseError;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct JsonDiagnostic {
    pub code: &'static str,
    pub level: &'static str,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub message: String,
    pub hint: Option<String>,
}

impl JsonDiagnostic {
    /// Stable code for type / semantic analyzer failures.
    pub const CODE_TYPE: &'static str = "E-TP01";
    /// Stable code for non-themed I/O / generic failures (file open,
    /// output write, unknown emit kind, etc.).
    pub const CODE_IO: &'static str = "E-IO";
    /// Stable code for `RuntimeError::LoadBlob`.
    pub const CODE_RT_LOAD_BLOB: &'static str = "E-RT01";
    /// Stable code for `RuntimeError::Assembly`.
    pub const CODE_RT_ASSEMBLY: &'static str = "E-RT02";
    /// Stable code for `crush_vm::AssemblyError` surfaced from
    /// `crush-compile` (the CLI fallback path that calls
    /// `crush_lang_sdk::assemble` directly rather than through
    /// `Runtime::run_casm`). Parallels `CODE_RT_ASSEMBLY` but applies to
    /// the standalone assembler surface so editors can route on the
    /// same family of errors regardless of which binary produced them.
    pub const CODE_ASSEMBLER: &'static str = "E-ASM";
    /// Stable code for `RuntimeError::IndexParse`.
    pub const CODE_RT_INDEX_PARSE: &'static str = "E-RT03";
    /// Stable code for `RuntimeError::IndexRead`.
    pub const CODE_RT_INDEX_READ: &'static str = "E-RT04";
    /// Stable code for `RuntimeError::Vm`.
    pub const CODE_RT_VM: &'static str = "E-RT05";

    /// Build a `JsonDiagnostic` from a parse error. Coordinates are
    /// always present for parse errors.
    pub fn parse_error(err: &ParseError, file: Option<&str>) -> Self {
        let (line, col, msg, code) = parse_error_triple(err);
        Self {
            code,
            level: "error",
            file: file.map(|s| s.to_string()),
            line: Some(line),
            col: Some(col),
            message: msg,
            hint: None,
        }
    }

    /// Build a `JsonDiagnostic` from a semantic / type-check error. The
    /// analyzer does not yet attach source ranges, so line/col are `null`.
    pub fn type_error(message: &str, file: Option<&str>) -> Self {
        let mut d = Self::generic_error(message, Self::CODE_TYPE);
        d.file = file.map(String::from);
        d
    }

    /// Build a `JsonDiagnostic` for a standalone assembler failure (e.g.
    /// `crush_lang_sdk::assemble` returning a `crush_vm::AssemblyError`
    /// from `crush-compile`). Mirrors `type_error`'s shape — file is
    /// attachable, source ranges are not surfaced here (callers that have
    /// a `&crush_vm::AssemblyError` can prefix `line N:` into the message
    /// themselves before invoking this constructor). Distinct stable code
    /// so editors can branch on `code == "E-ASM"` independently from the
    /// runtime-side `"E-RT02"` produced by `RuntimeError::Assembly`.
    pub fn assembler_error(message: &str, file: Option<&str>) -> Self {
        let mut d = Self::generic_error(message, Self::CODE_ASSEMBLER);
        d.file = file.map(String::from);
        d
    }

    /// Build a `JsonDiagnostic` for a non-themed failure (file I/O,
    /// unknown emit kind, missing-stage errors, etc.) that doesn't carry
    /// source coordinates. Use [`Self::CODE_IO`] for the common case.
    pub fn generic_error(message: &str, code: &'static str) -> Self {
        Self {
            code,
            level: "error",
            file: None,
            line: None,
            col: None,
            message: message.to_string(),
            hint: None,
        }
    }

    /// Map a [`crate::RuntimeError`] to a structured `JsonDiagnostic`.
    /// Each variant gets a distinct stable code so editors can route on the
    /// `code` field without parsing English. For [`crate::RuntimeError::Vm`]
    /// the variant uses `#[error(transparent)]`, so its `Display` text is
    /// identical to the inner `VmError`. We drop `message` to empty and put
    /// the full `VmError` display into `hint` so editors don't see the same
    /// word-for-word text in both fields.
    pub fn runtime_error(err: &crate::RuntimeError) -> Self {
        match err {
            crate::RuntimeError::LoadBlob(_) => Self {
                code: Self::CODE_RT_LOAD_BLOB,
                level: "error",
                file: None,
                line: None,
                col: None,
                message: err.to_string(),
                hint: None,
            },
            crate::RuntimeError::Assembly(_) => Self {
                code: Self::CODE_RT_ASSEMBLY,
                level: "error",
                file: None,
                line: None,
                col: None,
                message: err.to_string(),
                hint: None,
            },
            crate::RuntimeError::IndexParse { .. } => Self {
                code: Self::CODE_RT_INDEX_PARSE,
                level: "error",
                file: None,
                line: None,
                col: None,
                message: err.to_string(),
                hint: None,
            },
            crate::RuntimeError::IndexRead { .. } => Self {
                code: Self::CODE_RT_INDEX_READ,
                level: "error",
                file: None,
                line: None,
                col: None,
                message: err.to_string(),
                hint: None,
            },
            crate::RuntimeError::Vm(vm_err) => Self {
                code: Self::CODE_RT_VM,
                level: "error",
                file: None,
                line: None,
                col: None,
                // Empty to avoid duplicating `vm_err`'s display (the
                // `#[error(transparent)]` annotation makes `err.to_string()`
                // and `vm_err.to_string()` byte-identical).
                message: String::new(),
                hint: Some(vm_err.to_string()),
            },
        }
    }

    /// Serialize this diagnostic to a single line of JSON.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }
}

/// Render a slice of [`JsonDiagnostic`] as NDJSON (one complete JSON
/// object per line), terminated with a single trailing newline. NDJSON
/// is the wire format most editors / LSP bridges already accept.
pub fn render_diagnostics_ndjson(diags: &[JsonDiagnostic]) -> String {
    if diags.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for d in diags {
        out.push_str(&d.to_line());
        out.push('\n');
    }
    out
}

static INIT: Once = Once::new();

/// Severity levels used to pick a color for the leading `[…]` badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// Detect the terminal once for the lifetime of the process. Honors the
/// `NO_COLOR` convention and disables colors when stderr/stdout are not
/// attached to a terminal. Idempotent.
pub fn init_styling() {
    INIT.call_once(|| {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        // CRUSH_FORCE_COLOR=1 lets agents and tests force colors on even
        // when stdout is captured.
        let force = std::env::var_os("CRUSH_FORCE_COLOR")
            .map(|v| v == "1")
            .unwrap_or(false);
        let attached = std::io::stderr().is_terminal() || std::io::stdout().is_terminal();
        if no_color || (!attached && !force) {
            yansi::disable();
        }
    });
}

#[cfg(test)]
fn disable_for_test() {
    yansi::disable();
}

// --- colored string helpers ----------------------------------------------------

/// Wrap `text` in a severity-styled `[label]` badge followed by a single space.
fn paint_badge(sev: Severity, text: &str) -> String {
    let styled = match sev {
        Severity::Error => yansi::Paint::red(text).bold(),
        Severity::Warning => yansi::Paint::yellow(text).bold(),
        Severity::Note => yansi::Paint::blue(text).bold(),
    };
    format!("[{styled}]")
}

/// Convenience: `[error]`.
pub fn paint_error_badge(text: &str) -> String {
    paint_badge(Severity::Error, text)
}

/// Convenience: `[warning]`.
pub fn paint_warning_badge(text: &str) -> String {
    paint_badge(Severity::Warning, text)
}

/// Convenience: `[note]`.
pub fn paint_note_badge(text: &str) -> String {
    paint_badge(Severity::Note, text)
}

/// Cyan, used for file paths / locations.
pub fn paint_path(text: &str) -> String {
    yansi::Paint::cyan(text).to_string()
}

/// Bold green — for success / good / OK annotations and `=>` results.
pub fn paint_good(text: &str) -> String {
    yansi::Paint::green(text).bold().to_string()
}

/// Dim gray — for annotation marks like `-->` and gutter bars.
pub fn paint_dim(text: &str) -> String {
    yansi::Paint::dim(text).to_string()
}

/// Bold magenta — for the REPL prompt cadence.
pub fn paint_prompt(text: &str) -> String {
    yansi::Paint::magenta(text).bold().to_string()
}

// --- source snippet ----------------------------------------------------------

/// Format `source` as a numbered snippet with a caret pointing at `column`
/// on `line`. Optionally prepended with `-->` and `file` (a "labelled"
/// snippet, à la rustc).
///
/// Pure: no stderr side-effects, safe to test.
pub fn render_source_snippet(source: &str, line: usize, col: usize, file: Option<&str>) -> String {
    if source.is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    let target_idx = line.saturating_sub(1);
    if target_idx >= lines.len() {
        // Out-of-range; emit the `-->` header so callers still show the
        // claimed location, but skip the body to avoid spurious caret.
        if let Some(label) = file {
            return format!(
                "{arrow} {path}\n",
                arrow = paint_dim("-->"),
                path = paint_path(label),
            );
        }
        return String::new();
    }
    let width = line.max(lines.len()).to_string().len();

    let mut out = String::new();

    if let Some(label) = file {
        out.push_str(&format!(
            "{arrow} {path}\n",
            arrow = paint_dim("-->"),
            path = paint_path(label),
        ));
    }

    let start = target_idx.saturating_sub(2);
    let end = (target_idx + 3).min(lines.len());

    for (i, line_text) in lines.iter().enumerate().take(end).skip(start) {
        let marker = if i == target_idx { ">" } else { " " };
        let line_num = format!("{:>width$}", i + 1, width = width);
        let sep = paint_dim("|");
        let styled_num = if i == target_idx {
            yansi::Paint::yellow(line_num.as_str()).bold().to_string()
        } else {
            paint_dim(&line_num)
        };
        out.push_str(&format!(
            "{marker} {styled_num} {sep} {body}\n",
            marker = marker,
            styled_num = styled_num,
            sep = sep,
            body = line_text,
        ));
    }

    if target_idx >= start && target_idx < end {
        let col_idx = col.saturating_sub(1);
        let indent = " ".repeat(width + 1);
        let sep = paint_dim("|");
        let caret_at = " ".repeat(col_idx);
        let caret = yansi::Paint::red("^").bold().to_string();
        out.push_str(&format!("  {indent}{sep} {caret_at}{caret}\n"));
    }

    out
}

// --- parse errors ------------------------------------------------------------

/// Tuple of `(line, col, human-message, stable-code)` for a [`ParseError`].
/// Codes are stable `E-PPnn` identifiers so users can track a specific class
/// of error without grepping message substrings.
pub fn parse_error_triple(err: &ParseError) -> (usize, usize, String, &'static str) {
    match err {
        ParseError::UnexpectedToken { line, col, msg } => {
            (*line, *col, msg.clone(), "E-PP01")
        }
        ParseError::Expected {
            line,
            col,
            expected,
            found,
        } => (
            *line,
            *col,
            format!("expected {expected}, found {found}"),
            "E-PP02",
        ),
        ParseError::UnexpectedEOF { line, col } => (
            *line,
            *col,
            "unexpected end of input".to_string(),
            "E-PP03",
        ),
        ParseError::InvalidNumber { line, col, value } => (
            *line,
            *col,
            format!("invalid number literal `{value}`"),
            "E-PP04",
        ),
        ParseError::UnterminatedString { line, col } => (
            *line,
            *col,
            "unterminated string literal".to_string(),
            "E-PP05",
        ),
    }
}

/// Render a single [`ParseError`] as a multi-line, colored diagnostic with a
/// source snippet. Empty source still renders the header.
pub fn render_parse_error(err: &ParseError, file: Option<&str>, source: &str) -> String {
    let (line, col, msg, code) = parse_error_triple(err);
    let file_str = file.unwrap_or("<input>");
    let mut out = String::new();
    out.push_str(&format!(
        "{badge} {loc}: {msg}\n",
        badge = paint_error_badge(code),
        loc = paint_path(&format!("{file_str}:{line}:{col}")),
        msg = msg,
    ));
    let snippet = render_source_snippet(source, line, col, file);
    if !snippet.is_empty() {
        out.push_str(&snippet);
    }
    out
}

/// Render a slice of [`ParseError`]s back-to-back, separated by a blank line
/// and followed by an aggregate summary when there is more than one.
pub fn render_parse_errors(errors: &[ParseError], file: Option<&str>, source: &str) -> String {
    let mut out = String::new();
    for err in errors {
        out.push_str(&render_parse_error(err, file, source));
        out.push('\n');
    }
    let n = errors.len();
    if n > 1 {
        out.push_str(&format!(
            "{note}: aborting due to {n} previous errors\n",
            note = paint_note_badge("summary"),
        ));
    }
    out
}

// --- runtime errors ----------------------------------------------------------

/// Decorate any `Display`-able runtime/Vm/host error with a `[runtime]`
/// badge. Underlying `Display` text is preserved verbatim so existing
/// substring-based assertions keep passing.
pub fn render_runtime_error(err: &impl std::fmt::Display) -> String {
    format!(
        "{badge} {body}\n",
        badge = paint_error_badge("runtime"),
        body = err,
    )
}

/// Walk the `Error::source()` chain under a `[runtime]` badge. Same shape
/// as [`render_caused`] but with a runtime-specific label, signaling to the
/// reader that the failure originated in the VM/host runtime rather than
/// the build tool. Capped at 5 levels to avoid runaway pagination.
pub fn render_runtime_error_caused(error: &dyn std::error::Error) -> String {
    let mut out = format!(
        "{badge} {body}\n",
        badge = paint_error_badge("runtime"),
        body = error,
    );
    append_source_chain(error, &mut out);
    out
}

/// Render any `anyhow::Error` with its full cause chain using anyhow's own
/// `chain()` method. Works for `RuntimeError`, `std::io::Error`,
/// `crush_errors::CrushError`, and any other inner type — prefer this over
/// [`render_runtime_error_caused`] when the caller has `anyhow::Error`
/// because it does not require a downcast to detect the underlying type.
pub fn render_anyhow_error(e: &anyhow::Error, label: &str) -> String {
    let mut iter = e.chain();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut out = format!(
        "{badge} {first}\n",
        badge = paint_error_badge(label),
        first = first,
    );
    let mut depth = 0usize;
    for cause in iter {
        depth += 1;
        if depth > 5 {
            out.push_str(&format!(
                "  {arrow} (further chain elided)\n",
                arrow = paint_dim("..."),
            ));
            break;
        }
        out.push_str(&format!(
            "  {arrow} {cause}\n",
            arrow = paint_dim("Caused by:"),
            cause = cause,
        ));
    }
    out
}

/// Walk the `Error::source()` chain under a generic `[crush]` badge. The
/// first line is `error.to_string()`; subsequent lines are indented with a
/// dim `Caused by:` marker for clarity.
pub fn render_caused(error: &dyn std::error::Error) -> String {
    let mut out = format!(
        "{badge} {body}\n",
        badge = paint_error_badge("crush"),
        body = error,
    );
    append_source_chain(error, &mut out);
    out
}

fn append_source_chain(error: &dyn std::error::Error, out: &mut String) {
    let mut current = error.source();
    let mut depth = 0usize;
    while let Some(src) = current {
        depth += 1;
        if depth > 5 {
            out.push_str(&format!(
                "  {arrow} (further chain elided)\n",
                arrow = paint_dim("..."),
            ));
            break;
        }
        out.push_str(&format!(
            "  {arrow} {text}\n",
            arrow = paint_dim("Caused by:"),
            text = src,
        ));
        current = src.source();
    }
}

// --- REPL ergonomic shims ----------------------------------------------------

/// Colorize a REPL prompt token (e.g. `crush>`, `...>`).
pub fn format_repl_prompt(text: &str) -> String {
    paint_prompt(text)
}

/// Colorize the trailing `=> <value>` result marker on a successful eval.
pub fn format_repl_result(text: &str) -> String {
    format!(
        "{arrow} {value}",
        arrow = paint_dim("=>"),
        value = paint_good(text),
    )
}

// --- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crush_frontend::parser::ParseError;

    #[test]
    fn paint_error_badge_shape() {
        disable_for_test();
        let s = paint_error_badge("X");
        assert!(s.starts_with('[') && s.ends_with(']'));
        assert!(s.contains('X'));
    }

    #[test]
    fn render_source_snippet_single_line() {
        disable_for_test();
        let src = "let x = 42\n";
        let out = render_source_snippet(src, 1, 9, Some("a.crush"));
        assert!(out.contains("a.crush"));
        assert!(out.contains("let x = 42"));
        assert!(out.contains("-->"));
        assert!(out.contains('^'));
    }

    #[test]
    fn render_source_snippet_empty_source_returns_empty() {
        disable_for_test();
        // Empty source is a hard stop — caller must have at least one line.
        assert!(render_source_snippet("", 1, 1, None).is_empty());
    }

    #[test]
    fn render_source_snippet_out_of_range_returns_file_header_only() {
        disable_for_test();
        let src = "one\ntwo\nthree\n";
        let out = render_source_snippet(src, 99, 1, Some("a.crush"));
        // No body lines because target_idx > source lines, but the -->
        // header should still tell the user where the error was claimed.
        assert!(out.contains("a.crush"));
        assert!(out.contains("-->"));
        assert!(!out.contains("one"));
        assert!(!out.contains("two"));
        assert!(!out.contains("three"));
        // Without a file label, the function returns empty.
        assert!(render_source_snippet(src, 99, 1, None).is_empty());
    }

    #[test]
    fn parse_error_triple_canonical_codes() {
        let cases = [
            (ParseError::UnexpectedEOF { line: 3, col: 5 }, ("E-PP03", 3, 5)),
            (
                ParseError::UnterminatedString { line: 7, col: 2 },
                ("E-PP05", 7, 2),
            ),
            (
                ParseError::InvalidNumber {
                    line: 1,
                    col: 1,
                    value: "12abc".to_string(),
                },
                ("E-PP04", 1, 1),
            ),
        ];
        for (err, (code, line, col)) in cases {
            let (l, c, _msg, got_code) = parse_error_triple(&err);
            assert_eq!(l, line);
            assert_eq!(c, col);
            assert_eq!(got_code, code);
        }
    }

    #[test]
    fn assembler_error_constructor_lock() {
        // Lock `JsonDiagnostic::assembler_error`'s wire-format contract
        // so a future regression that drops the file attachment or
        // shifts the code/level defaults surfaces in CI before editors
        // see dup-schema records. Pairs with
        // `parse_error_triple_canonical_codes` (which locks the parse-
        // error canonical codes) and the external
        // `crush_compile_test::crush_compile_emits_json_diagnostic_for_assembler_error`
        // end-to-end test (which locks the conductor + stash plumbing).
        // This test covers only the constructor's data shape.
        let d = JsonDiagnostic::assembler_error(
            "line 3: duplicate label \"foo\"",
            Some("hello.casm"),
        );
        assert_eq!(d.code, JsonDiagnostic::CODE_ASSEMBLER);
        assert_eq!(
            d.code, "E-ASM",
            "E-ASM must be the wire code for assembler diagnostics"
        );
        assert_eq!(d.level, "error");
        assert_eq!(
            d.message, "line 3: duplicate label \"foo\"",
            "message must round-trip verbatim"
        );
        assert_eq!(
            d.file.as_deref(),
            Some("hello.casm"),
            "file must be preserved when Some"
        );
        assert!(d.line.is_none(), "assembler_error must not populate line");
        assert!(d.col.is_none(), "assembler_error must not populate col");
        assert!(d.hint.is_none(), "assembler_error must not populate hint");

        // File == None branch: same shape, just no file attached.
        let d_no_file = JsonDiagnostic::assembler_error("line 1: bad opcode", None);
        assert_eq!(d_no_file.code, "E-ASM");
        assert_eq!(d_no_file.level, "error");
        assert_eq!(d_no_file.message, "line 1: bad opcode");
        assert!(
            d_no_file.file.is_none(),
            "file must be None when caller passes None"
        );
        assert!(d_no_file.line.is_none());
        assert!(d_no_file.col.is_none());
        assert!(d_no_file.hint.is_none());
    }

    #[test]
    fn render_parse_error_unexpected_eof() {
        disable_for_test();
        let err = ParseError::UnexpectedEOF { line: 1, col: 1 };
        let out = render_parse_error(&err, Some("a.crush"), "fn main() {\n");
        assert!(out.contains("[E-PP03]"));
        assert!(out.contains("a.crush:1:1"));
        assert!(out.contains("unexpected end of input"));
    }

    #[test]
    fn render_parse_errors_aggregate_summary() {
        disable_for_test();
        let errors = vec![
            ParseError::UnexpectedEOF { line: 1, col: 1 },
            ParseError::UnterminatedString { line: 2, col: 3 },
        ];
        let out = render_parse_errors(&errors, Some("a.crush"), "fn main() {\n");
        assert!(out.contains("[E-PP03]"));
        assert!(out.contains("[E-PP05]"));
        assert!(out.contains("aborting due to 2 previous errors"));
    }

    #[test]
    fn render_runtime_error_preserves_display_text() {
        disable_for_test();
        struct Sentinel(&'static str);
        impl std::fmt::Display for Sentinel {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }
        let rendered = render_runtime_error(&Sentinel("instruction quota exceeded (10)"));
        assert!(rendered.contains("[runtime]"));
        assert!(rendered.contains("instruction quota exceeded (10)"));
    }

    #[test]
    fn render_caused_walks_chain() {
        disable_for_test();
        let root = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let wrapped = anyhow::Error::new(root).context("building program");
        // unwrap the box, then introspect the chain.
        let msg = format!("{wrapped:#}");
        let out = render_caused(&*wrapped.source().unwrap());
        assert!(out.contains("[crush]"));
        // The chain walk should surface at least the immediate error text.
        assert!(out.contains("file missing") || out.contains("building program"));
        // Substring preserved somewhere in either the rendered output or the chain message.
        let _ = msg; // keep lint happy
    }

    #[test]
    fn render_runtime_error_caused_uses_runtime_badge() {
        disable_for_test();
        // std types that DO impl std::error::Error — `anyhow::Error`
        // deliberately does not (so it can own the downcast).
        let root = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let out = render_runtime_error_caused(&root);
        // [runtime] badge identifies the layer; the underlying message survives.
        assert!(out.contains("[runtime]"));
        assert!(out.contains("access denied"));
    }

    #[test]
    fn format_repl_decorations_no_panic() {
        disable_for_test();
        assert!(format_repl_prompt("crush> ").contains("crush>"));
        assert!(format_repl_result("42").contains("42"));
        assert!(format_repl_result("42").contains("=>"));
    }

    #[test]
    fn init_styling_is_idempotent() {
        // Should not panic when called repeatedly.
        init_styling();
        init_styling();
        init_styling();
    }
}
