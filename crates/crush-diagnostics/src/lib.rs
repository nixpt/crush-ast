//! Canonical NDJSON diagnostic helper trio for the Crush CLI surface.
//!
//! # Wire format
//!
//! Every diagnostic is one NDJSON line (newline-terminated) with
//! seven fixed fields in this exact order:
//!
//! `code, level, file, line, col, message, hint`
//!
//! Field positions are pinned by [`DiagRecord`]'s struct-declaration
//! order (serde-derived `Serialize` preserves declaration order) AND
//! by [`diag_line`]'s byte-exact output. The canonical wire contract
//! also lives in `crush_lang_sdk::theme::JsonDiagnostic` (used by the
//! four CLI binaries `crush`, `crushc`, `crush-run`, `crush-compile`,
//! `crush-repl`); [`DiagRecord`] mirrors that shape so editors see one
//! schema across the entire CLI surface.
//!
//! # Helpers
//!
//! - [`DiagRecord`] — public struct, serde-derived. Feed it into
//!   [`diag_line`] to get the NDJSON string.
//! - [`diag_line`] — returns the NDJSON line (with trailing `\n`).
//! - [`diag_line_from`] — function-form shortcut that constructs a
//!   [`DiagRecord`] inline; produces byte-identical output to
//!   [`diag_line`]. Useful at narrow call sites where constructing a
//!   struct would be over-ceremony (e.g. `&format!("{e:#}")`
//!   temporaries, where Rust's lifetime-extension only kicks in for
//!   function arguments).
//! - [`wants_json`] — flag parser that recognizes
//!   `--message-format=json`, `--message-format json`, and
//!   `--message-format-json` at any position.
//! - [`strict_downgrade`] — the strict-mode kernel: lifts
//!   `level: "note"` → `level: "error"` under strict mode so
//!   non-fatal warnings become build-failures (CI gate). Hoisted
//!   out of `crush-pkg::main` so any binary adopting
//!   `--message-format=strict` (xtask, crush-vm,
//!   crush-installer, future crush-lint over .dejavue) routes
//!   through one canonical implementation rather than forking
//!   the kernel into N per-binary `*::diag` modules.
//!
//! # Stream routing
//!
//! This crate does NOT expose a `print!`/`eprint!` helper. Callers
//! explicitly choose the stream:
//!
//! - `xtask` audit / lint-dejavue write errors to `stderr` (matches
//!   ripgrep-style behaviour).
//! - `crush-vm` writes errors to `stderr`, informational `note`
//!   warnings to `stderr` too (standalone emulator).
//! - `crush-installer` / `crush-pkg` write success/info NDJSON to
//!   `stdout` so editor consumers can pipe a single stream;
//!   corresponding non-JSON `eprintln!` traffic stays on `stderr`.
//!
//! Pinning the stream at the crate level would break one class or
//! the other; the call site is the right place for that decision.
//!
//! # Deserialization (consumer side)
//!
//! The [`wire_consumer`] submodule offers two deserialization paths:
//!
//! - **Owned** (default): [`wire_consumer::OwnedDiagRecord`],
//!   [`wire_consumer::parse_record`], [`wire_consumer::consume_stream`]
//!   — always allocates `String` per field; records outlive the input
//!   buffer. Use for streaming from pipes/sockets where each line is
//!   transient.
//! - **Borrowed** (zero-copy hot path):
//!   [`wire_consumer::BorrowedDiagRecord`],
//!   [`wire_consumer::parse_record_borrowed`],
//!   [`wire_consumer::consume_stream_borrowed`] — uses `Cow<'a, str>`
//!   with `#[serde(borrow)]` so unescaped JSON strings borrow directly
//!   from the input (zero allocation); escaped strings fall back to
//!   `Cow::Owned` (one allocation, same as owned). Use when the caller
//!   owns a long-lived buffer (e.g. `mmap`ed file, cached response).
//!
//! Both paths share the same seven-field wire shape and round-trip
//! with [`diag_line`]. This makes the crate bidirectional — serialize
//! on the emitter side, deserialize on the consumer side — so a
//! wire-shape change touches one crate, not N. Previously this parser
//! lived inlined in `crush-debugger`; consolidating it here eliminates
//! that duplicate wire-shape copy.

pub mod wire_consumer;

// Re-export the deserialization surface at the crate root so callers
// can `use crush_diagnostics::{OwnedDiagRecord, parse_record, ...}`
// without drilling into the submodule. `crush-debugger` re-exports
// these same names from its own root for back-compat with its existing
// call sites.
pub use wire_consumer::{
    consume_stream, consume_stream_borrowed, parse_record, parse_record_borrowed,
    BorrowedDiagRecord, OwnedDiagRecord, ParseRecordError,
};

/// Seven-field wire-shape mirror of
/// `crush_lang_sdk::theme::JsonDiagnostic`. The field ORDER is the
/// load-bearing invariant: every binary calls `serde_json::to_string`
/// on a `DiagRecord` so the JSON object key order matches the
/// canonical declaration order, and the lockdown tests in
/// `tests/wire_format.rs` enforce byte-exact equality.
///
/// Field types are kept minimal: `level: &'a str` (fixed enum),
/// `code: &'a str` (per-binary inline literal), `file/line/col`
/// optional (attach-per-constructor — see `JsonDiagnostic`'s
/// attachability table in `crush_lang_sdk/src/theme.rs`), and
/// `message/hint` as raw text (serde applies RFC 8259 §7 escaping
/// internally; no double-encoding at the call site).
#[derive(serde::Serialize)]
pub struct DiagRecord<'a> {
    pub code: &'a str,
    pub level: &'a str,
    pub file: Option<&'a str>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub message: &'a str,
    pub hint: Option<&'a str>,
}

/// Render one NDJSON line (newline-terminated). Wraps
/// `serde_json::to_string`'s never-fails output (the struct owns all
/// statically-typed fields; no enum or `Map<NonString, _>`).
///
/// Callers explicitly print to stdout or stderr as appropriate —
/// `print!("{}", diag_line(&rec))` or `eprint!("{}", diag_line(&rec))`
/// keeps the stream-routing decision at the call site (this is why
/// the crate does not expose an `emit_diag` helper).
pub fn diag_line(rec: &DiagRecord<'_>) -> String {
    let mut s = serde_json::to_string(rec)
        .expect("DiagRecord fields are statically typed; serde_json::to_string cannot fail");
    s.push('\n');
    s
}

/// Function-form shortcut for callers that don't want to construct
/// a `DiagRecord` themselves. Byte-identical output to
/// `diag_line(&DiagRecord { code, level, file, line: None, col: None, message, hint })`.
///
/// Pin via `diag_line_from_byte_equals_diag_line` in
/// `tests/wire_format.rs`. Useful for short call sites that pass
/// `&format!("{e:#}")` temporaries (lifetime-extension only kicks in
/// at function-call arguments, not at struct-literal RHS).
pub fn diag_line_from<'a>(
    code: &'a str,
    level: &'a str,
    message: &'a str,
    hint: Option<&'a str>,
    file: Option<&'a str>,
) -> String {
    diag_line(&DiagRecord {
        code,
        level,
        file,
        line: None,
        col: None,
        message,
        hint,
    })
}

/// True iff one of `args` is `--message-format=json` (or
/// space-separated `--message-format json` or single-dash
/// `--message-format-json`). Hand-parsed; each consuming binary
/// decides whether to pull `clap` for this flag (those with `clap`
/// use clap's `global = true` derive; those without — `xtask` and
/// `crush-vm` — use this hand-rolled parser to keep the dep graph
/// small for CI tooling and the standalone VM).
///
/// Recognizes the flag at *any* position relative to the subcommand
/// (e.g. `xtask --message-format=json audit` AND
/// `xtask audit --message-format=json`). This emulates the
/// cruft-tolerant behavior clap provides by default for global flags.
pub fn wants_json(args: &[String]) -> bool {
    let mut iter = args.iter().peekable();
    while let Some(a) = iter.next() {
        if a == "--message-format=json" || a == "--message-format-json" {
            return true;
        }
        if a == "--message-format"
            && let Some(next) = iter.peek()
            && next.as_str() == "json"
        {
            return true;
        }
    }
    false
}

/// Apply the strict-mode level-downgrade kernel: a
/// `level: "note"` record becomes `level: "error"` under strict
/// mode (so the warning class breaks the build via the existing
/// exit-1-on-error path); other levels pass through unchanged.
///
/// # Design
///
/// Both arms return borrowed `&str` (zero allocation). The
/// passthrough arm re-borrows from the input `level`; the lifted
/// arm returns the canonical `"error"` static literal. The
/// `'static` lifetime coerces to any input lifetime via Rust's
/// reference covariance, so the public return type is the plain
/// `&str` (matching the canonical surface style — no `Cow`
/// wrapper). Caller just threads `&final_level` into a
/// `DiagRecord::level`.
///
/// # CI gate rationale
///
/// Used by every binary that adopts `--message-format=strict` so
/// build-time lint warnings (e.g. `crush-pkg`'s future
/// dead-code-in-`capsule.toml` detector) become hard build
/// failures under CI. Centralizing here means the rule lives in
/// one place: a future binary that adds strict mode just calls
/// `strict_downgrade(level, strict_mode)` rather than re-deriving
/// the note→error lift (which would silently diverge if a future
/// contributor tightened the rule differently in N forks).
///
/// Edge case `level == "warning"` under strict mode: this helper
/// does NOT escalate warnings to errors; the only canonical lift
/// is `note → error`. Warnings stay warnings so CI still sees
/// the offending record (with a navigation hint) but doesn't
/// break the build. If a future policy decision escalates
/// warnings too, the change is a single edit here rather than
/// a scattered fork-fix across binaries.
pub fn strict_downgrade(level: &str, strict_mode: bool) -> &str {
    if strict_mode && level == "note" {
        "error"
    } else {
        level
    }
}

// Sanity tests for the encoder live inside the crate's `private_quotes`
// sub-module below; the PUBLIC lockdown consolidation lives in
// `tests/wire_format.rs`. (Tests on private helpers can't be exposed
// through integration tests without leaking them as `pub`.)
//
// (The `hinted_text` 4 KiB cap — used by `xtask audit` for the
// `rg.stderr` overflow case — lives privately in `xtask/src/diag.rs`
// rather than here; this crate stays minimal.)

#[cfg(test)]
mod private_quotes {
    // Sanity-check the encoding arms we'll need if we ever expose a
    // hint-cap helper. The wire-format lockdown tests in
    // `tests/wire_format.rs` exercise the same paths via the serde
    // layer; this is a fast in-crate check that the codepoints
    // round-trip cleanly.
    #[test]
    fn quote_escape_round_trip_through_serde() {
        for s in [
            "hello",
            "with \"quote\"",
            "line\nbreak",
            "back\\slash",
            "tab\there",
            "\u{0001}",
            "☃",
        ] {
            let rec = super::DiagRecord {
                code: "E-IO",
                level: "error",
                file: None,
                line: None,
                col: None,
                message: s,
                hint: None,
            };
            let line = super::diag_line(&rec);
            let v: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
            assert_eq!(v["message"], s, "quote escape broke for input: {s:?}");
        }
    }
}
