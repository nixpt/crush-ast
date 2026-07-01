//! Parse the `emit_post_dispatch_lint` NDJSON stream emitted by
//! `crush-pkg::emit_post_dispatch_lint` (MessageFormat::Json) into owned
//! records surfacable from a long-running debugger session.
//!
//! `crush_diagnostics::DiagRecord<'a>` is `Serialize`-only (zero-copy on
//! emit). On the consume side we need owned `String` fields, so we map
//! each line to [`OwnedDiagRecord`] — same field order, same JSON
//! shape, but owned.
//!
//! The round-trip test below pins the parser against the canonical
//! emitter in `crush-pkg::handle_lint_with_byte_exact_three_rule_fedpath`
//! so a wire-shape change must touch both crates together.

use std::io::{BufRead, Read};

use crush_diagnostics::DiagRecord;
use serde_json::Value;

/// Owned, parsed NDJSON diagnostic record. Field order mirrors
/// `DiagRecord::serialize` — do NOT reshuffle without updating the
/// wire-format lockdown tests in `crush-pkg::main::tests`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crush_diagnostics::DiagRecord;

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
        let text = crush_diagnostics::diag_line(&rec);
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
}
