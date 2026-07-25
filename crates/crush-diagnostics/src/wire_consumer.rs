//! Parse the NDJSON diagnostic stream emitted by `diag_line` /
//! `diag_line_from` back into owned records.
//!
//! [`crate::DiagRecord<'a>`] is `Serialize`-only (zero-copy on emit).
//! On the consume side callers need owned `String` fields, so each
//! NDJSON line is mapped to [`OwnedDiagRecord`] — same field order,
//! same JSON shape, but owned.
//!
//! This module is the canonical home for the parser: it was previously
//! inlined in `crush-debugger/src/wire_consumer.rs`. `crush-debugger`
//! now re-exports it from here so a wire-shape change touches one crate,
//! not two. The round-trip tests below pin the parser against the
//! canonical emitter (`diag_line`) so a wire-shape change must touch
//! both the serializer and the deserializer together.

use std::borrow::Cow;
use std::io::{BufRead, Read};

use crate::DiagRecord;
use serde_json::Value;

/// Owned, parsed NDJSON diagnostic record. Field order mirrors
/// `DiagRecord::serialize` — do NOT reshuffle without updating the
/// wire-format lockdown tests in `tests/wire_format.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedDiagRecord {
    pub code: String,
    pub level: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    pub message: String,
    pub hint: Option<String>,
}

/// Convert a borrowed `DiagRecord` into an owned `OwnedDiagRecord`.
impl<'a> From<&DiagRecord<'a>> for OwnedDiagRecord {
    fn from(r: &DiagRecord<'a>) -> Self {
        Self {
            code: r.code.to_string(),
            level: r.level.to_string(),
            file: r.file.map(str::to_string),
            line: r.line,
            col: r.col,
            message: r.message.to_string(),
            hint: r.hint.map(str::to_string),
        }
    }
}

/// Why a single NDJSON line didn't survive validation.
#[derive(Debug)]
pub enum ParseRecordError {
    /// Top-level JSON value isn't a JSON object.
    NotAnObject,
    /// serde_json parser failure.
    Json(serde_json::Error),
    /// Required `&str` field missing or not a string.
    MissingString(&'static str),
    /// Optional `Option<&str>` field present but not a string or null.
    BadOptionalString(&'static str),
    /// Optional `Option<u32>` field present but not a number or null.
    BadOptionalNumber(&'static str),
}

impl std::fmt::Display for ParseRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnObject => f.write_str("top-level JSON value is not an object"),
            Self::Json(e) => write!(f, "NDJSON parse failed: {e}"),
            Self::MissingString(name) => {
                write!(f, "required string field `{name}` missing or non-string")
            }
            Self::BadOptionalString(name) => {
                write!(f, "optional string field `{name}` present but not string/null")
            }
            Self::BadOptionalNumber(name) => {
                write!(f, "optional number field `{name}` present but not number/null")
            }
        }
    }
}

impl std::error::Error for ParseRecordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

/// Parse a single NDJSON line into an `OwnedDiagRecord`.
/// Field order matches `DiagRecord::serialize`:
///   code, level, file, line, col, message, hint
pub fn parse_record(line: &str) -> Result<OwnedDiagRecord, ParseRecordError> {
    let v: Value = serde_json::from_str(line.trim()).map_err(ParseRecordError::Json)?;
    let obj = v.as_object().ok_or(ParseRecordError::NotAnObject)?;
    let req_str = |name: &'static str| -> Result<String, ParseRecordError> {
        obj.get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(ParseRecordError::MissingString(name))
    };
    let opt_str = |name: &'static str| -> Result<Option<String>, ParseRecordError> {
        match obj.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            _ => Err(ParseRecordError::BadOptionalString(name)),
        }
    };
    let opt_u32 = |name: &'static str| -> Result<Option<u32>, ParseRecordError> {
        match obj.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .map(Some)
                .ok_or(ParseRecordError::BadOptionalNumber(name)),
            _ => Err(ParseRecordError::BadOptionalNumber(name)),
        }
    };
    Ok(OwnedDiagRecord {
        code: req_str("code")?,
        level: req_str("level")?,
        file: opt_str("file")?,
        line: opt_u32("line")?,
        col: opt_u32("col")?,
        message: req_str("message")?,
        hint: opt_str("hint")?,
    })
}

/// Consume an NDJSON stream from a `Read` source. Yields one
/// `OwnedDiagRecord` per non-empty, non-whitespace-only line.
/// Blank lines are silently skipped (common in piped CI output).
pub fn consume_stream<R: Read>(
    reader: R,
) -> impl Iterator<Item = Result<OwnedDiagRecord, ParseRecordError>> {
    let buf = std::io::BufReader::new(reader);
    buf.lines().filter_map(|line_result| {
        line_result.ok().and_then(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(parse_record(trimmed))
            }
        })
    })
}

// ----------------------------------------------------------------
// Zero-copy borrowed deserialization (hot-path alternative)
// ----------------------------------------------------------------
//
// `OwnedDiagRecord` always allocates a `String` per field. For hot-
// path consumers reading from a buffer they already own (e.g. a file
// mapped into memory, an HTTP response body), `BorrowedDiagRecord<'a>`
// uses `Cow<'a, str>` with `#[serde(borrow)]` so that unescaped JSON
// strings borrow directly from the input (zero allocation, zero copy),
// while escaped strings fall back to `Cow::Owned` (one allocation,
// same as `OwnedDiagRecord`). This is the standard serde zero-copy-
// when-possible pattern.
//
// `DiagRecord<'a>` (the emit-side struct) uses plain `&'a str` fields
// and cannot be used for deserialization — `&str` would ERROR on any
// escaped string. `BorrowedDiagRecord<'a>` sidesteps that by using
// `Cow<'a, str>`, which gracefully handles both cases. It derives both
// `Serialize` (byte-identical wire shape to `DiagRecord` / `OwnedDiagRecord`)
// and `Deserialize` (zero-copy when possible).
//
// The stream variant (`consume_stream_borrowed`) takes a `&'a [u8]`
// slice rather than an `impl Read` — borrowed records can only live as
// long as the buffer they borrow from, so the caller must own the
// buffer. This is the right API for the hot-path use case.

/// Zero-copy borrowed diagnostic record. Same seven-field wire shape
/// as [`DiagRecord`] and [`OwnedDiagRecord`], but with `Cow<'a, str>`
/// fields so serde can borrow directly from the input JSON when the
/// string contains no escape sequences (zero allocation) and fall
/// back to `Cow::Owned` when it does (one allocation, same as
/// `OwnedDiagRecord`).
///
/// Derives both `Serialize` (byte-identical output to `DiagRecord`)
/// and `Deserialize` (zero-copy when possible via `#[serde(borrow = "'a")]`).
///
/// # Zero-copy scope
///
/// The **required** string fields (`code`, `level`, `message`) achieve
/// true zero-copy: unescaped JSON strings borrow directly from the
/// input buffer as `Cow::Borrowed`, with pointer equality to the source.
///
/// The **optional** string fields (`file`, `hint`) are wrapped in
/// `Option<Cow<'a, str>>`, and serde_json's current deserializer does
/// not propagate borrows through the `Option` wrapper — these fields
/// always allocate as `Cow::Owned` even for unescaped strings. This is
/// a known serde_json limitation, not a bug in this crate. The
/// `#[serde(borrow = "'a")]` attribute is kept on all fields for
/// correctness and forward-compatibility (if serde_json fixes the
/// `Option<Cow>` path, these fields will automatically become zero-copy).
///
/// Escaped strings (containing `\"`, `\n`, `\uXXXX`, etc.) always
/// fall back to `Cow::Owned` on ALL fields — serde must allocate to
/// hold the unescaped result. This is the graceful-degradation path
/// that plain `&str` fields cannot survive (they would error).
///
/// Use this when you have a long-lived buffer (e.g. `mmap`ed file,
/// `Vec<u8>` you own) and want to avoid per-field `String` allocation
/// on the required fields. Use [`OwnedDiagRecord`] when you need the
/// records to outlive the input buffer (e.g. streaming from a pipe
/// where each line is transient).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct BorrowedDiagRecord<'a> {
    #[serde(borrow = "'a")]
    pub code: Cow<'a, str>,
    #[serde(borrow = "'a")]
    pub level: Cow<'a, str>,
    #[serde(borrow = "'a")]
    pub file: Option<Cow<'a, str>>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    #[serde(borrow = "'a")]
    pub message: Cow<'a, str>,
    #[serde(borrow = "'a")]
    pub hint: Option<Cow<'a, str>>,
}

/// Promote a borrowed record to an owned one (for when the caller needs
/// the record to outlive the input buffer). Each `Cow::Borrowed` becomes
/// a `String::from` (one allocation per field); `Cow::Owned` is already
/// a `String` and is moved out cheaply.
impl<'a> From<&BorrowedDiagRecord<'a>> for OwnedDiagRecord {
    fn from(r: &BorrowedDiagRecord<'a>) -> Self {
        Self {
            code: r.code.to_string(),
            level: r.level.to_string(),
            file: r.file.as_ref().map(|c| c.to_string()),
            line: r.line,
            col: r.col,
            message: r.message.to_string(),
            hint: r.hint.as_ref().map(|c| c.to_string()),
        }
    }
}

/// Parse a single NDJSON line into a `BorrowedDiagRecord<'a>`, borrowing
/// directly from `line` when the JSON strings contain no escape
/// sequences. Escaped strings fall back to `Cow::Owned` (allocation).
///
/// The returned record's borrows are tied to `line`'s lifetime — the
/// caller must keep `line` alive as long as the record is in use.
pub fn parse_record_borrowed(line: &str) -> Result<BorrowedDiagRecord<'_>, ParseRecordError> {
    serde_json::from_str(line.trim()).map_err(ParseRecordError::Json)
}

/// Consume an NDJSON stream from a borrowed byte slice. Yields one
/// `BorrowedDiagRecord<'a>` per non-empty, non-whitespace-only line,
/// each borrowing directly from `data` when possible (zero-copy for
/// unescaped strings). Blank lines are silently skipped.
///
/// The returned records borrow from `data` — the caller must keep
/// `data` alive as long as the records are in use. This is the hot-
/// path API for consumers that already own the buffer (e.g. `mmap`ed
/// files, cached response bodies). For streaming from a `Read` source
/// where each line is transient, use [`consume_stream`] (owned) instead.
///
/// # Limitations
///
/// - If `data` is not valid UTF-8, the entire stream is silently
///   dropped (zero records yielded, no error). The owned
///   [`consume_stream`] handles invalid UTF-8 per-line. For the hot-
///   path use case (JSON from `mmap`/cache), the input is virtually
///   always valid UTF-8.
/// - Optional string fields (`file`, `hint`) always allocate due to
///   serde_json's `Option<Cow>` limitation (see [`BorrowedDiagRecord`]).
pub fn consume_stream_borrowed<'a>(
    data: &'a [u8],
) -> impl Iterator<Item = Result<BorrowedDiagRecord<'a>, ParseRecordError>> {
    let text = std::str::from_utf8(data).unwrap_or("");
    text.split('\n').filter_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(parse_record_borrowed(trimmed))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagRecord;

    /// Round-trip a hand-authored `DiagRecord` through `diag_line` and
    /// back via `parse_record`. Mirrors the byte-exact fedpath lockdown
    /// in `crush-pkg::handle_lint_with_byte_exact_three_rule_fedpath`.
    #[test]
    fn parse_record_roundtrips_diag_line_emitted_canonical_note_record() {
        let rec = DiagRecord {
            code: "E-BUILDER",
            level: "note",
            file: Some("capsule.toml"),
            line: Some(7),
            col: None,
            message: "placeholder value `TEMP` must be filled in",
            hint: Some("set TEMP in your shell before running"),
        };
        let text = crate::diag_line(&rec);
        let parsed = parse_record(&text).expect("must parse a canonical emitter line");
        let expected = OwnedDiagRecord::from(&rec);
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_record_rejects_non_object_top_level() {
        let err = parse_record("[1,2,3]").unwrap_err();
        assert!(matches!(err, ParseRecordError::NotAnObject));
    }

    #[test]
    fn parse_record_rejects_missing_required_string() {
        let err = parse_record(r#"{"level":"note","message":"x"}"#).unwrap_err();
        assert!(matches!(err, ParseRecordError::MissingString("code")));
    }

    #[test]
    fn parse_record_rejects_non_numeric_optional_number() {
        let err = parse_record(
            r#"{"code":"E","level":"note","file":null,"line":"seven","col":null,"message":"m","hint":null}"#,
        )
        .unwrap_err();
        assert!(matches!(err, ParseRecordError::BadOptionalNumber("line")));
    }

    #[test]
    fn consume_stream_skips_blank_lines_and_yields_records() {
        let input = b"\n{\"code\":\"E-BUILDER\",\"level\":\"note\",\"file\":null,\"line\":null,\"col\":null,\"message\":\"first\",\"hint\":null}\n\n\n{\"code\":\"E-BUILDER\",\"level\":\"note\",\"file\":\"x.crush\",\"line\":1,\"col\":2,\"message\":\"second\",\"hint\":null}\n";
        let records: Vec<OwnedDiagRecord> = consume_stream(&input[..])
            .collect::<Result<Vec<_>, _>>()
            .expect("all non-blank lines must parse");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].message, "first");
        assert_eq!(records[0].file, None);
        assert_eq!(records[1].file.as_deref(), Some("x.crush"));
        assert_eq!(records[1].line, Some(1));
        assert_eq!(records[1].col, Some(2));
    }

    // ── BorrowedDiagRecord / parse_record_borrowed / consume_stream_borrowed ──

    /// Zero-copy evidence: for unescaped JSON strings, the REQUIRED
    /// `Cow` fields (code, level, message) must be `Borrowed` and point
    /// INTO the original line buffer (same pointer). The OPTIONAL fields
    /// (file, hint) are wrapped in `Option<Cow>` and serde_json does not
    /// propagate borrows through `Option` — those allocate as `Owned`.
    /// This is a known serde_json limitation (documented on
    /// `BorrowedDiagRecord`); the values are still correct, just not
    /// zero-copy. If serde_json fixes the `Option<Cow>` path, the
    /// `#[serde(borrow = "'a")]` attribute is already in place to
    /// enable zero-copy automatically.
    #[test]
    fn parse_record_borrowed_zero_copy_for_unescaped_strings() {
        let line = r#"{"code":"E-LINT","level":"error","file":"src/x.rs","line":42,"col":7,"message":"site boundary","hint":"fix it"}"#;
        let rec = parse_record_borrowed(line).expect("unescaped line must parse");

        // Required fields: must be Borrowed (zero-copy).
        assert!(matches!(rec.code, Cow::Borrowed(_)), "code must be Borrowed");
        assert!(matches!(rec.level, Cow::Borrowed(_)), "level must be Borrowed");
        assert!(matches!(rec.message, Cow::Borrowed(_)), "message must be Borrowed");

        // Optional fields: serde_json doesn't propagate borrows through
        // Option<Cow>, so these are Owned (allocated). Values are still
        // correct — just not zero-copy. Assert correctness, not Borrowed.
        assert_eq!(rec.file.as_deref(), Some("src/x.rs"));
        assert_eq!(rec.hint.as_deref(), Some("fix it"));

        // Pointer equality on a required field: the borrowed slice must
        // point into `line`. If serde copied, the pointer would differ.
        let code_ptr = rec.code.as_ptr();
        let line_code_start = line.find("\"E-LINT\"").unwrap() + 1; // skip opening quote
        assert_eq!(
            code_ptr, line[line_code_start..].as_ptr(),
            "code must point into the input buffer (zero-copy)"
        );
    }

    /// Escaped strings fall back to Cow::Owned (allocation, but no error).
    /// This is the graceful-degradation path that &str would not survive.
    #[test]
    fn parse_record_borrowed_falls_back_to_owned_for_escaped_strings() {
        // message contains an escaped quote: "has \"quote\""
        let line = r#"{"code":"E-LINT","level":"error","file":null,"line":null,"col":null,"message":"has \"quote\"","hint":null}"#;
        let rec = parse_record_borrowed(line).expect("escaped line must parse (not error)");

        // Unescaped fields still borrow.
        assert!(matches!(rec.code, Cow::Borrowed(_)), "code must be Borrowed");
        assert!(matches!(rec.level, Cow::Borrowed(_)), "level must be Borrowed");

        // Escaped field must be Owned (serde un-escaped it into a new String).
        assert!(
            matches!(rec.message, Cow::Owned(_)),
            "message with escapes must be Owned (allocated), not Borrowed"
        );
        assert_eq!(rec.message, "has \"quote\"");
    }

    /// Round-trip: DiagRecord → diag_line → parse_record_borrowed →
    /// BorrowedDiagRecord. Field values must match the original.
    #[test]
    fn parse_record_borrowed_roundtrips_diag_line() {
        let rec = DiagRecord {
            code: "E-BUILDER",
            level: "note",
            file: Some("capsule.toml"),
            line: Some(7),
            col: None,
            message: "placeholder value `TEMP` must be filled in",
            hint: Some("set TEMP in your shell before running"),
        };
        let text = crate::diag_line(&rec);
        let parsed = parse_record_borrowed(&text).expect("must parse a canonical emitter line");

        assert_eq!(parsed.code, rec.code);
        assert_eq!(parsed.level, rec.level);
        assert_eq!(parsed.file.as_deref(), Some("capsule.toml"));
        assert_eq!(parsed.line, Some(7));
        assert_eq!(parsed.col, None);
        assert_eq!(parsed.message, rec.message);
        assert_eq!(parsed.hint.as_deref(), Some("set TEMP in your shell before running"));
    }

    /// BorrowedDiagRecord → OwnedDiagRecord conversion: all fields
    /// survive the promotion, including Cow::Borrowed → String.
    #[test]
    fn borrowed_record_promotes_to_owned() {
        let line = r#"{"code":"E-LINT","level":"error","file":"f.rs","line":1,"col":2,"message":"msg","hint":"h"}"#;
        let borrowed = parse_record_borrowed(line).expect("parse");
        let owned = OwnedDiagRecord::from(&borrowed);

        assert_eq!(owned.code, "E-LINT");
        assert_eq!(owned.level, "error");
        assert_eq!(owned.file.as_deref(), Some("f.rs"));
        assert_eq!(owned.line, Some(1));
        assert_eq!(owned.col, Some(2));
        assert_eq!(owned.message, "msg");
        assert_eq!(owned.hint.as_deref(), Some("h"));
    }

    /// Stream-level zero-copy: consume_stream_borrowed yields records
    /// that borrow from the input slice, skipping blank lines.
    #[test]
    fn consume_stream_borrowed_skips_blanks_and_yields_borrowed_records() {
        let input = b"\n{\"code\":\"E-A\",\"level\":\"error\",\"file\":null,\"line\":null,\"col\":null,\"message\":\"first\",\"hint\":null}\n\n{\"code\":\"E-B\",\"level\":\"note\",\"file\":\"x.rs\",\"line\":3,\"col\":null,\"message\":\"second\",\"hint\":null}\n";
        let records: Vec<BorrowedDiagRecord<'_>> = consume_stream_borrowed(&input[..])
            .collect::<Result<Vec<_>, _>>()
            .expect("all non-blank lines must parse");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].code, "E-A");
        assert_eq!(records[0].message, "first");
        assert_eq!(records[1].file.as_deref(), Some("x.rs"));
        assert_eq!(records[1].line, Some(3));
        // Zero-copy evidence on the stream: code must be Borrowed.
        assert!(matches!(records[0].code, Cow::Borrowed(_)));
    }
}
