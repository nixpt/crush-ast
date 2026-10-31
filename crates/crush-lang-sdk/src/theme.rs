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
//! # `JsonDiagnostic` NDJSON wire format
//!
//! The CLI binaries (`crushc`, `crush-run`, `crush-compile`,
//! `crush-repl`) emit error and warning events as **one NDJSON
//! line per record** on stderr when invoked with
//! `--message-format json`. This section is the stable contract
//! for that stream — embedders should be able to route on the
//! `code` field alone without parsing English `message` text.
//!
//! ## Canonical code table
//!
//! Codes are the routing field. They're organised into families
//! so a future `E-PP06`, `E-RT06`, etc. can be appended without
//! breaking prefix-based routing.
//!
//! | Family        | Code         | Emitted by                                  |
//! |---------------|--------------|---------------------------------------------|
//! | Parse errors  | `E-PP01..05` | `parse_error_triple` (inline `&'static str`; not on `JsonDiagnostic::CODE_*`) |
//! | Type / sema   | `E-TP01`     | `JsonDiagnostic::CODE_TYPE`                 |
//! | Capability    | `E-CAPS`     | `JsonDiagnostic::CODE_CAPS` (`crush-run`: program needs a capability the run does not grant; `crush-pkg check`: declared-vs-used findings) |
//! | Runtime load  | `E-RT01`     | `JsonDiagnostic::CODE_RT_LOAD_BLOB`         |
//! | Runtime asm   | `E-RT02`     | `JsonDiagnostic::CODE_RT_ASSEMBLY`         |
//! | Runtime idx-p | `E-RT03`     | `JsonDiagnostic::CODE_RT_INDEX_PARSE`       |
//! | Runtime idx-r | `E-RT04`     | `JsonDiagnostic::CODE_RT_INDEX_READ`        |
//! | Runtime VM    | `E-RT05`     | `JsonDiagnostic::CODE_RT_VM`                |
//! | CLI assembler | `E-ASM`      | `JsonDiagnostic::CODE_ASSEMBLER`            |
//! | Generic I/O   | `E-IO`       | `JsonDiagnostic::CODE_IO`                   |
//! | Import        | `E-IM01`     | `JsonDiagnostic::CODE_IMPORT`               |
//!
//! Unlike the runtime-side codes (`E-RT*`, `E-TP*`, `E-IO`,
//! `E-ASM`) which live as `pub const CODE_*` on `JsonDiagnostic`,
//! the parse-error codes `E-PP01..05` are emitted as inline
//! `&'static str` literals inside `parse_error_triple`. Both
//! shapes are equally stable; embedders should match on a prefix
//! rather than reach for an individual constant that may not
//! exist.
//!
//! ## Codes emitted by CLI surfaces outside the four Crush binaries
//!
//! The Crush CI toolchain has additional emitters that re-use the
//! NDJSON shape above so editors see a uniform stream regardless
//! of which CLI produced the record. These extend the inline-literal
//! convention (parallel to `E-PP01..05`) and live as `const` in
//! each binary's `main.rs` rather than on `JsonDiagnostic`:
//!
//! | Code         | Emitted by                                         |
//! |--------------|----------------------------------------------------|
//! | `E-AUDIT`    | `xtask/src/main.rs::run_audit` (cargo xtask audit)  |
//! | `E-LINT`     | `xtask/src/lint_dejavue.rs::main` (cargo xtask lint-dejavue) |
//!
//! Each emitter threads `json_mode: bool` (hand-rolled, no clap dep +
//! no `crush-lang-sdk` dep) and emits via a local `json_diag_line`
//! helper whose seven-field shape is locked by a `json_diag_line_field_order_is_canonical`
//! unit test in the binary's `mod tests`. The lockdown test is what
//! keeps the wire format in lock-step with the canonical struct here:
//! a contributor who reorders derive-field order on `JsonDiagnostic`
//! breaks BOTH the canonical test in this file AND the per-binary
//! lockdown tests in `xtask/src/main.rs::tests`,
//! `xtask/src/lint_dejavue.rs::tests`, and
//! `crush-vm/src/main.rs::tests`.
//!
//! ## Level convention
//!
//! `level` is always one of `"error"`, `"warning"`, or `"note"`,
//! matching the [`Severity`] enum. Today every wire record emits
//! `"error"`; `"warning"` and `"note"` are reserved for future
//! lint / hint surfaces. Editors should accept any of the three —
//! newer levels will land as additional `Severity` variants with
//! matching lowercase strings.
//!
//! ## Field-shape invariants
//!
//! - `code` — always present, stable per the table above.
//! - `level` — always present, one of `{"error", "warning", "note"}`.
//! - `message` — always present; non-empty for typed errors
//!   EXCEPT the `RuntimeError::Vm` arm (see dedupe below).
//! - `file` — the source (or program) the record is about, whenever a CLI
//!   knows it: every `crushc` and `crush-run` record carries it.
//! - `line` / `col` — 1-based; `Some(N)` for syntax errors (always) and for
//!   semantic errors (whenever the analyzer can name the construct:
//!   undefined variable / function, type mismatches, arity). `None` for
//!   failures that are not about a place in the source: I/O, a refused
//!   capability, and runtime errors (the VM does not map a failing
//!   instruction back to a source line yet).
//! - `hint` — optional advice. Never the error text itself: that is always
//!   `message`. (Before CRUSH-315 the `E-RT05` record left `message` empty and
//!   put the text in `hint`.) `E-CAPS` from `crush-run` uses it for the grant
//!   flag (`grant with --fs`).
//!
//! ## One input, one record
//!
//! `crushc` and `crush-run` produce byte-identical records for a program that
//! fails to compile (syntax: `E-PP0n`, one per error; semantic: `E-TP01`), via
//! [`diagnostics_for_error`]. `crush-run` additionally reports what only a run
//! can see: `E-CAPS` (before anything runs) and `E-RT0n` (while it runs). The
//! corpus in `tests/diagnostics_corpus/` and `tests/diagnostics_golden.rs`
//! lock this.
//!
//! ## Attachability per constructor
//!
//! | Constructor                       | file       | line | col  | hint |
//! |-----------------------------------|------------|------|------|------|
//! | `parse_error`                     | caller-set | Some | Some | None |
//! | `semantic_error`                  | caller-set | when known | when known | None |
//! | `type_error`                      | caller-set | None | None | None |
//! | `generic_error`                   | caller-set (`in_file`) | None | None | None |
//! | `assembler_error`                 | caller-set | None | None | None |
//! | `runtime_error` (all variants)    | caller-set (`in_file`) | None | None | None |
//!
//! ## Stability guarantees
//!
//! Codes and `level` strings are stable across minor releases;
//! new fields MAY be appended to the record; existing fields are
//! NEVER removed or repurposed. Today's wire can be parsed by
//! tomorrow's clients without a schema-version handshake.
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

/// The NDJSON diagnostic record. Defined once, in `crush-diagnostics`
/// ([`crush_diagnostics::Diagnostic`]); this alias keeps the name the CLIs and
/// embedders have always used. The constructors that need this crate's or the
/// parser's types ([`parse_error`], [`runtime_error`], [`semantic_error`]) are
/// on [`JsonDiagnosticExt`] (and exist as free functions here), which is
/// implemented for the alias so `JsonDiagnostic::parse_error(..)` keeps
/// working wherever the trait is in scope (CRUSH-315).
///
/// [`parse_error`]: JsonDiagnosticExt::parse_error
/// [`runtime_error`]: JsonDiagnosticExt::runtime_error
/// [`semantic_error`]: JsonDiagnosticExt::semantic_error
pub type JsonDiagnostic = crush_diagnostics::Diagnostic;

/// Constructors for [`JsonDiagnostic`] that depend on types this crate owns.
pub trait JsonDiagnosticExt: Sized {
    /// Build a record from a parse error. Coordinates are always present.
    fn parse_error(err: &ParseError, file: Option<&str>) -> Self;

    /// Build a record from a semantic / type-check error, with its position
    /// when the analyzer attached one (`file` is attached either way).
    fn semantic_error(err: &crush_frontend::SemanticError, file: Option<&str>) -> Self;

    /// Map a [`crate::RuntimeError`] to a structured record. Each variant gets
    /// a distinct stable code so editors can route on `code` without parsing
    /// English. The error text is always in `message` (CRUSH-315: the
    /// `RuntimeError::Vm` arm used to leave it empty and put the text in
    /// `hint`).
    fn runtime_error(err: &crate::RuntimeError) -> Self;

    /// Build an `E-IM01` record from an unresolved import, in the file the
    /// problem is in (CRUSH-110).
    fn import_error(err: &crush_frontend::import_system::ImportError) -> Self;
}

impl JsonDiagnosticExt for JsonDiagnostic {
    fn parse_error(err: &ParseError, file: Option<&str>) -> Self {
        let (line, col, msg, code) = parse_error_triple(err);
        JsonDiagnostic::error(code, msg).in_file(file).at(line, col)
    }

    fn semantic_error(err: &crush_frontend::SemanticError, file: Option<&str>) -> Self {
        let d = JsonDiagnostic::type_error(&err.message, file);
        match err.pos {
            Some((line, col)) => d.at(line, col),
            None => d,
        }
    }

    fn import_error(err: &crush_frontend::import_system::ImportError) -> Self {
        let d = JsonDiagnostic::error(JsonDiagnostic::CODE_IMPORT, &err.message)
            .in_file(Some(&err.file));
        match err.pos {
            Some((line, col)) => d.at(line, col),
            None => d,
        }
    }

    fn runtime_error(err: &crate::RuntimeError) -> Self {
        let code = match err {
            crate::RuntimeError::LoadBlob(_) => JsonDiagnostic::CODE_RT_LOAD_BLOB,
            crate::RuntimeError::Assembly(_) => JsonDiagnostic::CODE_RT_ASSEMBLY,
            crate::RuntimeError::IndexParse { .. } => JsonDiagnostic::CODE_RT_INDEX_PARSE,
            crate::RuntimeError::IndexRead { .. } => JsonDiagnostic::CODE_RT_INDEX_READ,
            crate::RuntimeError::Vm(_) => JsonDiagnostic::CODE_RT_VM,
        };
        JsonDiagnostic::error(code, err.to_string())
    }
}

/// The records for a compile or run failure that is not a bare
/// [`crate::RuntimeError`], shared by `crushc` and `crush-run` so the same
/// broken program yields the same records from both (CRUSH-315):
///
/// - syntax errors ([`crush_frontend::ParseErrors`]): one `E-PP0n` record per
///   error, with position;
/// - semantic errors ([`crush_frontend::SemanticError`]): one `E-TP01` record,
///   with position when the analyzer knows it;
/// - a typed `RuntimeError`: its `E-RT0n` record;
/// - anything else: `fallback`.
///
/// `file` is the source path, attached to every record.
pub fn diagnostics_for_error(
    err: &anyhow::Error,
    file: Option<&str>,
    fallback: &'static str,
) -> Vec<JsonDiagnostic> {
    if let Some(parse) = err.downcast_ref::<crush_frontend::ParseErrors>() {
        return parse
            .0
            .iter()
            .map(|e| JsonDiagnostic::parse_error(e, file))
            .collect();
    }
    if let Some(sema) = err.downcast_ref::<crush_frontend::SemanticError>() {
        // An error inside an imported module names that module's file.
        return vec![JsonDiagnostic::semantic_error(sema, sema.file.as_deref().or(file))];
    }
    if let Some(imp) = err.downcast_ref::<crush_frontend::import_system::ImportError>() {
        return vec![JsonDiagnostic::import_error(imp)];
    }
    if let Some(rt) = err.downcast_ref::<crate::RuntimeError>() {
        return vec![JsonDiagnostic::runtime_error(rt).in_file(file)];
    }
    vec![JsonDiagnostic::generic_error(&err.to_string(), fallback).in_file(file)]
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
        // CRUSH-27 added `UnknownAnnotation` to the ParseError enum
        // (emitted by the parser when it sees a bare `@<unknown>` form).
        // Render it with `E-PP06` so downstream consumers (CLI
        // diagnostics, vscode extension squiggles) can target it
        // alongside the existing `E-PP01..05`. Update the canonical
        // code-table comment at the top of `theme.rs` (and extend
        // `parse_error_triple_canonical_codes` tests below) when more
        // variants land.
        ParseError::UnknownAnnotation { name, line, col } => (
            *line,
            *col,
            format!("unknown annotation @{name}"),
            "E-PP06",
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
