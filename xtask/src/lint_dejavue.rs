// xtask/src/lint_dejavue.rs
//
// Future-preventive: any RESOLVED-named event in `.dejavue/timeline.jsonl`
// that lacks an explicit supersession marker within 1 hour is flagged.
//
// Triggered by the multi-iteration (a/b/c modified) rustls workspace-build
// fix debug cycle, where an implicit-via-chronology correction was missed
// because nothing in the workspace flagged it on write.  Per code-reviewer
// point #2: every `RESOLVED` claim needs an explicit follow-up marker.
//
// Usage:
//   cargo run -p xtask --bin lint-dejavue [path/to/timeline.jsonl]
//                          [--message-format=json]
//   (defaults to `.dejavue/timeline.jsonl`)
//
// Wire shape: `--message-format=json` emits one NDJSON record per event on
// stderr, mirroring `crush_lang_sdk::theme::JsonDiagnostic` (code, level,
// file, line, col, message, hint).  Lint failures use code `E-LINT`;
// file-I/O failures use code `E-IO` (the canonical generic code).
//
// All NDJSON helpers + canonical lockdown tests live in `xtask::diag`
// (consumed via `use xtask::diag::...`); this binary is now a thin
// shell that wires the lint-condition logic onto shared emitters.

use std::fs;
use std::path::Path;
use std::process::ExitCode;
use std::sync::OnceLock;

use regex::Regex;

use xtask::diag::{hinted_text, diag_line_from, wants_json, CODE_IO, CODE_LINT};

// =====================================================================
// Constants
// =====================================================================

/// Window within which a RESOLVED event must have a companion supersession
/// marker (`<event>_superseded` suffix OR `supersedes_<event>` prefix) to
/// avoid being flagged.  1 hour per user spec.
pub const WINDOW_SECONDS: i64 = 3600;

// =====================================================================
// Types
// =====================================================================

#[derive(Debug, Clone)]
pub struct Event {
    pub ts: String,
    pub event: String,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub resolved_event: String,
    pub resolved_ts: String,
    pub candidate_form: String,
    pub reason: String,
}

// =====================================================================
// JSON field extraction (regex-based, escape-aware)
// =====================================================================
//
// Pattern matches `"key" : "value"` pairs in flat JSON objects.  The
// value pattern `((?:[^"\\]|\\.)*)` handles escapes so values containing
// `\"`, `\\`, `\n`, `\t` etc. are captured intact rather than truncated
// at the first literal quote (the regex-v1 limitation surfaced by the
// code-reviewer).

static FIELD_PATTERN: OnceLock<Regex> = OnceLock::new();

fn field_pattern() -> &'static Regex {
    FIELD_PATTERN.get_or_init(|| {
        let pattern = r#""([a-zA-Z_][a-zA-Z0-9_]*)"\s*:\s*"((?:[^"\\]|\\.)*)""#;
        Regex::new(pattern).expect("FIELD_PATTERN compiles")
    })
}

/// Lookup the string value of a top-level field by key.  Returns None if
/// the field is absent on this line or the line isn't parseable.
pub fn extract_field(line: &str, key: &str) -> Option<String> {
    for caps in field_pattern().captures_iter(line) {
        if let (Some(k), Some(v)) = (caps.get(1), caps.get(2)) {
            if k.as_str() == key {
                return Some(unescape(v.as_str()));
            }
        }
    }
    None
}

/// Expand JSON-style escape sequences in a string literal.  Used to
/// convert regex-captured `\"` back to `"`, etc., so that `==` and
/// `contains` comparisons on extracted values match what a JSON
/// decoder would yield.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('b') => out.push('\u{0008}'),
                Some('f') => out.push('\u{000C}'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => {
                    out.push('\\');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// =====================================================================
// Resolution-type classifier
// =====================================================================

/// True iff `event` is the *type* of RESOLVED-named event the lint flags.
/// Acceptable forms (must satisfy ALL of):
/// - contains the substring "resolved"
/// - does NOT end with "_superseded"  (already-superseded suffix form)
/// - does NOT contain "_final"         (terminal claim form)
/// - does NOT start with "supersedes_" (the supersession marker itself)
pub fn is_resolved_type_for_lint(event: &str) -> bool {
    event.contains("resolved")
        && !event.ends_with("_superseded")
        && !event.contains("_final")
        && !event.starts_with("supersedes_")
}

// =====================================================================
// RFC3339 timestamp parsing (offset-aware, std-only)
// =====================================================================
//
// Format: `YYYY-MM-DDTHH:MM:SS[.ffffff][Z|(+|-)HH:MM]`.  We avoid the
// `chrono` crate (not in xtask's deps) and use Howard Hinnant's
// days_from_civil for date arithmetic.

/// Parse an RFC3339 timestamp into Unix seconds.  Returns None on bad input.
pub fn time_to_unix_seconds(ts: &str) -> Option<i64> {
    let t_pos = ts.find('T')?;
    let date = &ts[..t_pos];
    let rest = &ts[t_pos + 1..];

    // ---- date ----
    let mut date_parts = date.split('-');
    let y: i64 = date_parts.next()?.parse().ok()?;
    let mo: i64 = date_parts.next()?.parse().ok()?;
    let d: i64 = date_parts.next()?.parse().ok()?;

    // ---- time: HH:MM:SS ----
    // Find positions of first two colons to slice HH:MM:SS off cleanly.
    let mut first_colon = None;
    let mut second_colon = None;
    for (i, c) in rest.char_indices() {
        if c == ':' {
            if first_colon.is_none() {
                first_colon = Some(i);
            } else if second_colon.is_none() {
                second_colon = Some(i);
                break;
            }
        }
    }
    let first_colon = first_colon?;
    let second_colon = second_colon?;
    let h_str = &rest[..first_colon];
    let m_str = &rest[first_colon + 1..second_colon];
    let after_seconds = &rest[second_colon + 1..];
    let s_str_end = after_seconds
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_seconds.len());
    let s_str = &after_seconds[..s_str_end];

    let h: i64 = h_str.parse().ok()?;
    let m: i64 = m_str.parse().ok()?;
    let s: i64 = s_str.parse().ok()?;

    // ---- offset (defaults to 0 if absent; Z == +0; explicit +HH:MM / -HH:MM) ----
    let after_s = &after_seconds[s_str_end..];
    let offset_seconds: i64 = if after_s.starts_with('Z') || after_s.starts_with('z') {
        0
    } else if after_s.starts_with('+') || after_s.starts_with('-') {
        let sign: i64 = if after_s.starts_with('-') { -1 } else { 1 };
        let tail = &after_s[1..];
        let om_pos = tail.find(':');
        let oh_str = match om_pos {
            Some(p) => &tail[..p],
            None => tail,
        };
        let om_str = match om_pos {
            Some(p) => &tail[p + 1..],
            None => "0",
        };
        let oh: i64 = oh_str.parse().ok()?;
        let om: i64 = if om_str.is_empty() { 0 } else { om_str.parse().ok()? };
        sign * (oh * 3600 + om * 60)
    } else {
        0
    };

    let days = days_from_civil(y, mo, d);
    Some(days * 86400 + h * 3600 + m * 60 + s - offset_seconds)
}

/// Howard Hinnant's days_from_civil (well-known, correct; days from
/// 1970-01-01).  https://howardhinnant.github.io/date_algorithms.html
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y_adj = if m <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// =====================================================================
// Window check
// =====================================================================

/// True iff `ts_after` is within `window_seconds` of `ts_before` AND
/// `ts_after >= ts_before`.  Backwards returns false (no negative
/// windows).
pub fn is_within_window(ts_before: &str, ts_after: &str, window_seconds: i64) -> bool {
    let s_before = time_to_unix_seconds(ts_before);
    let s_after = time_to_unix_seconds(ts_after);
    match (s_before, s_after) {
        (Some(b), Some(a)) => a >= b && (a - b) <= window_seconds,
        _ => false,
    }
}

// =====================================================================
// Linting
// =====================================================================

/// Lint a JSONL timeline file.  Returns a sorted list of violations.
pub fn lint(path: &Path) -> Result<Vec<Violation>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("read {}: {}", path.display(), e))?;
    let mut entries: Vec<Event> = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let ts = match extract_field(line, "ts") {
            Some(s) => s,
            None => continue,
        };
        let event = match extract_field(line, "event") {
            Some(s) => s,
            None => continue,
        };
        entries.push(Event { ts, event });
    }

    let mut violations = Vec::new();
    for entry in &entries {
        if !is_resolved_type_for_lint(&entry.event) {
            continue;
        }
        // Accept BOTH `<event>_superseded` (suffix) AND `supersedes_<event>` (prefix).
        let candidate_suffix = format!("{}_superseded", entry.event);
        let candidate_prefix = format!("supersedes_{}", entry.event);
        let has_companion = entries.iter().any(|other| {
            (other.event == candidate_suffix || other.event == candidate_prefix)
                && is_within_window(&entry.ts, &other.ts, WINDOW_SECONDS)
        });
        if has_companion {
            continue;
        }
        let present_outside_window = entries.iter().any(|other| {
            other.event == candidate_suffix || other.event == candidate_prefix
        });
        let reason = if present_outside_window {
            format!(
                "candidate `{}` (or `{}`) is present in the timeline but OUTSIDE the {}-second window. \
                 Move/supersede the candidate within the window or rename to a `_final` form.",
                candidate_suffix, candidate_prefix, WINDOW_SECONDS
            )
        } else {
            format!(
                "no `{}` (or `{}`) companion exists for this RESOLVED-named event. \
                 Add an explicit supersession marker if this RESOLVED claim was later invalidated.",
                candidate_suffix, candidate_prefix
            )
        };
        violations.push(Violation {
            resolved_event: entry.event.clone(),
            resolved_ts: entry.ts.clone(),
            candidate_form: format!("{} | {}", candidate_suffix, candidate_prefix),
            reason,
        });
    }

    violations.sort_by(|a, b| a.resolved_ts.cmp(&b.resolved_ts));
    Ok(violations)
}

// =====================================================================
// Entry point
// =====================================================================

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json_mode = wants_json(&args);
    let path_str = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| ".dejavue/timeline.jsonl".to_string());
    let path = Path::new(&path_str);
    if !path.exists() {
        if json_mode {
            let msg = format!("{} does not exist", path.display());
            eprint!("{}", diag_line_from(CODE_IO, "error", &msg, None, None));
        } else {
            eprintln!("error: {} does not exist", path.display());
        }
        return ExitCode::from(2);
    }
    match lint(path) {
        Ok(violations) if violations.is_empty() => {
            if json_mode {
                let msg = "OK: timeline passes lint (no premature RESOLVED claims without supersession markers)";
                eprint!("{}", diag_line_from(CODE_LINT, "note", msg, None, None));
            } else {
                println!("OK: timeline passes lint (no premature RESOLVED claims without supersession markers)");
            }
            ExitCode::SUCCESS
        }
        Ok(violations) => {
            if json_mode {
                let summary = format!("FAIL: {} violation(s)", violations.len());
                eprint!("{}", diag_line_from(CODE_LINT, "error", &summary, None, None));
                for v in &violations {
                    let msg = format!(
                        "ts={} event={} candidate=\"{}\"",
                        v.resolved_ts, v.resolved_event, v.candidate_form
                    );
                    // Symmetric to xtask/src/main.rs::run_audit: cap
                    // the per-violation reason at HINT_MAX_BYTES so a
                    // dejavue timeline with super-long narrative
                    // reasons (e.g. a contributor pasting full
                    // snippets into the `reason` field) doesn't
                    // bloat the NDJSON consumer past the 64KiB pipe
                    // buffer.
                    let reason_capped = hinted_text(&v.reason);
                    eprint!(
                        "{}",
                        diag_line_from(CODE_LINT, "error", &msg, Some(&reason_capped), None)
                    );
                }
            } else {
                eprintln!("FAIL: {} violation(s):", violations.len());
                for v in &violations {
                    eprintln!("  - ts={} event={}", v.resolved_ts, v.resolved_event);
                    eprintln!("    candidate={}", v.candidate_form);
                    eprintln!("    {}", v.reason);
                }
            }
            ExitCode::from(1)
        }
        Err(e) => {
            if json_mode {
                eprint!("{}", diag_line_from(CODE_IO, "error", &e, None, None));
            } else {
                eprintln!("error: {}", e);
            }
            ExitCode::from(2)
        }
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_field_handles_known_keys() {
        let line = r#"{"ts":"2026-06-19T22:35:00-05:00","event":"workspace_rustls_resolved","agent":"codebuff"}"#;
        assert_eq!(
            extract_field(line, "ts").as_deref(),
            Some("2026-06-19T22:35:00-05:00")
        );
        assert_eq!(
            extract_field(line, "event").as_deref(),
            Some("workspace_rustls_resolved")
        );
        assert_eq!(extract_field(line, "agent").as_deref(), Some("codebuff"));
    }

    #[test]
    fn extract_field_handles_whitespace() {
        assert_eq!(extract_field(r#"{ "ts" :  "v" }"#, "ts").as_deref(), Some("v"));
    }

    #[test]
    fn extract_field_preserves_embedded_escapes() {
        // Summary contains escaped quotes -- regex must NOT terminate at first `\"`.
        let line = r#"{"ts":"x","event":"y","summary":"contains \"quoted\" text"}"#;
        assert_eq!(
            extract_field(line, "summary").as_deref(),
            Some(r#"contains "quoted" text"#)
        );
    }

    #[test]
    fn extract_field_missing_key_returns_none() {
        assert_eq!(extract_field(r#"{"event":"x"}"#, "ts"), None);
    }

    #[test]
    fn extract_field_no_match_returns_none() {
        assert_eq!(extract_field("not json", "ts"), None);
    }

    #[test]
    fn unescape_handles_sequences() {
        assert_eq!(unescape(r#""hello""#), r#""hello""#);
        assert_eq!(unescape(r"hello\nworld"), "hello\nworld");
        assert_eq!(unescape(r"\\path"), "\\path");
    }

    #[test]
    fn is_resolved_type_classifies() {
        assert!(is_resolved_type_for_lint("workspace_rustls_resolved"));
        assert!(is_resolved_type_for_lint("foo_resolved_bar"));
        assert!(
            !is_resolved_type_for_lint("workspace_rustls_resolved_superseded"),
            "suffix excluded"
        );
        assert!(
            !is_resolved_type_for_lint("workspace_rustls_resolved_v3_final"),
            "_final excluded"
        );
        assert!(
            !is_resolved_type_for_lint("supersedes_workspace_rustls_resolved"),
            "prefix excluded"
        );
        assert!(
            !is_resolved_type_for_lint("workspace_rustls_investigated"),
            "no 'resolved' substring"
        );
    }

    #[test]
    fn time_to_unix_seconds_offset_correctness() {
        // Same instant expressed in different offsets must produce same Unix seconds.
        let a = time_to_unix_seconds("2026-06-19T00:00:00-05:00").unwrap();
        let b = time_to_unix_seconds("2026-06-19T05:00:00Z").unwrap();
        let c = time_to_unix_seconds("2026-06-19T05:00:00+00:00").unwrap();
        let d = time_to_unix_seconds("2026-06-19T06:00:00+01:00").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(c, d);
    }

    #[test]
    fn time_to_unix_seconds_real_dates() {
        // 2026-06-19T22:35:00-05:00 -- verify it's in the right ballpark.
        let s = time_to_unix_seconds("2026-06-19T22:35:00-05:00").unwrap();
        assert!(s > 1_700_000_000 && s < 1_900_000_000, "got {}", s);
    }

    #[test]
    fn time_to_unix_seconds_bad_input() {
        assert_eq!(time_to_unix_seconds("2026-06-19"), None);
        assert_eq!(time_to_unix_seconds("not-a-ts"), None);
        assert_eq!(time_to_unix_seconds(""), None);
        assert_eq!(time_to_unix_seconds("2026-06-19T"), None);
    }

    #[test]
    fn is_within_window_bounds() {
        assert!(is_within_window(
            "2026-06-19T22:35:00-05:00",
            "2026-06-19T22:36:00-05:00",
            3600
        ));
        assert!(
            is_within_window(
                "2026-06-19T22:35:00-05:00",
                "2026-06-19T23:35:00-05:00",
                3600
            ),
            "exactly 1h"
        );
        assert!(
            !is_within_window(
                "2026-06-19T22:35:00-05:00",
                "2026-06-19T23:36:00-05:00",
                3600
            ),
            "just past 1h"
        );
        assert!(
            !is_within_window(
                "2026-06-19T22:35:00-05:00",
                "2026-06-19T22:34:00-05:00",
                3600
            ),
            "before"
        );
    }

    #[test]
    fn is_within_window_cross_day() {
        assert!(
            is_within_window(
                "2026-06-19T23:59:30-05:00",
                "2026-06-20T00:00:30-05:00",
                3600
            ),
            "1min past midnight"
        );
        assert!(
            !is_within_window(
                "2026-06-19T23:00:00-05:00",
                "2026-06-20T01:00:00-05:00",
                3600
            ),
            "2h past midnight"
        );
    }

    #[test]
    fn lint_empty_timeline_passes() {
        let p = std::env::temp_dir().join("lint-empty.jsonl");
        std::fs::write(&p, "").unwrap();
        assert!(lint(&p).unwrap().is_empty());
    }

    #[test]
    fn lint_resolved_with_superseded_suffix_companion_passes() {
        let p = std::env::temp_dir().join("lint-suffix-ok.jsonl");
        std::fs::write(
            &p,
            "{\"ts\":\"2026-06-19T22:35:00-05:00\",\"event\":\"foo_resolved\"}\n{\"ts\":\"2026-06-19T22:36:00-05:00\",\"event\":\"foo_resolved_superseded\"}\n",
        )
        .unwrap();
        assert!(lint(&p).unwrap().is_empty());
    }

    #[test]
    fn lint_resolved_with_supersedes_prefix_companion_passes() {
        let p = std::env::temp_dir().join("lint-prefix-ok.jsonl");
        std::fs::write(
            &p,
            "{\"ts\":\"2026-06-19T22:35:00-05:00\",\"event\":\"foo_resolved\"}\n{\"ts\":\"2026-06-19T22:36:00-05:00\",\"event\":\"supersedes_foo_resolved\"}\n",
        )
        .unwrap();
        assert!(lint(&p).unwrap().is_empty());
    }

    #[test]
    fn lint_resolved_without_companion_fails() {
        let p = std::env::temp_dir().join("lint-no-companion.jsonl");
        std::fs::write(
            &p,
            "{\"ts\":\"2026-06-19T22:35:00-05:00\",\"event\":\"foo_resolved\"}\n",
        )
        .unwrap();
        let v = lint(&p).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].resolved_event, "foo_resolved");
    }

    #[test]
    fn lint_final_excluded() {
        let p = std::env::temp_dir().join("lint-final.jsonl");
        std::fs::write(
            &p,
            "{\"ts\":\"2026-06-19T22:35:00-05:00\",\"event\":\"foo_resolved_v3_final\"}\n",
        )
        .unwrap();
        assert!(lint(&p).unwrap().is_empty(), "_final must NOT be flagged");
    }

    #[test]
    fn lint_companion_outside_window_fails() {
        let p = std::env::temp_dir().join("lint-outside-window.jsonl");
        std::fs::write(
            &p,
            "{\"ts\":\"2026-06-19T22:35:00-05:00\",\"event\":\"foo_resolved\"}\n{\"ts\":\"2026-06-19T23:36:00-05:00\",\"event\":\"foo_resolved_superseded\"}\n",
        )
        .unwrap();
        let v = lint(&p).unwrap();
        assert_eq!(v.len(), 1, "companion just outside 1h window must fail");
    }

    #[test]
    fn current_real_timeline_passes() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set");
        let p = std::path::PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .join(".dejavue/timeline.jsonl");
        if !p.exists() {
            panic!("smoke test: dejavue timeline missing at {}", p.display());
        }
        let v = lint(&p).unwrap();
        assert!(
            v.is_empty(),
            "current real timeline should pass lint; got violations: {:?}",
            v
        );
    }

    // ----------------------------------------------------------------
    // import-smoke tests for xtask::diag
    // ----------------------------------------------------------------
    //
    // Full wire-format lockdown lives in `xtask::diag::tests`. The
    // tests here merely confirm the import path resolves; a future
    // contributor who re-routes the import to a wrong module fails
    // these tests immediately rather than at the next CI run.

    #[test]
    fn diag_import_resolves() {
        assert_eq!(CODE_LINT, "E-LINT");
        assert_eq!(CODE_IO, "E-IO");
        assert!(wants_json(&[
            "lint-dejavue".to_string(),
            "--message-format=json".to_string(),
        ]));
        assert!(!wants_json(&["lint-dejavue".to_string()]));
    }

    #[test]
    fn diag_line_from_via_diag_module_emits_canonical_prefix() {
        let line = diag_line_from(CODE_LINT, "error", "smoke", None, None);
        assert!(
            line.starts_with(r#"{"code":"E-LINT","level":"error","file":null,"line":null,"col":null,"message":"smoke","hint":null}"#),
            "imported diag_line_from must emit the canonical seven-field shape (got: {line:?})"
        );
    }
}
