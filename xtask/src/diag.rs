//! NDJSON diagnostic helpers for the `xtask` package.
//!
//! Public re-exports from the canonical [`crush_diagnostics`] peer
//! crate. The seven-field wire shape (`code, level, file, line, col,
//! message, hint`) is mirrored across the entire CLI surface; this
//! module's only job is to surface xtask's per-binary wire codes +
//! the audit/lint codepath's local [`hinted_text`] cap helper.
//!
//! Canonical wire-format lockdown (byte-exact field order, embedded-
//! quote round-trip, canonical-order assertion) lives entirely in
//! `crates/crush-diagnostics/tests/wire_format.rs`. This module's
//! `mod tests` only carries the xtask-private [`hinted_text`] cap
//! tests + minimal import-smoke.
//!
//! [`crush_diagnostics`]: https://github.com/nixpt/crush-ast/tree/main/crates/crush-diagnostics

// Re-exports from the canonical peer crate. The audit + lint-dejavue
// binaries import through this module rather than from
// `crush_diagnostics` directly so a future rename / split of the
// canonical crate is a single-file change.
pub use crush_diagnostics::{diag_line, diag_line_from, wants_json, DiagRecord};

/// Audit wire code — emitted by the `cargo xtask audit` subcommand.
pub const CODE_AUDIT: &str = "E-AUDIT";

/// Lint wire code — emitted by the `cargo xtask lint-dejavue` binary.
pub const CODE_LINT: &str = "E-LINT";

/// Generic I/O wire code — file not found, malformed input, etc.
/// Mirrors `crush_lang_sdk::theme::JsonDiagnostic::CODE_IO`.
pub const CODE_IO: &str = "E-IO";

// =========================================================================
// Xtask-private hint-byte-cap helper.
//
// `hinted_text` is xtask-specific (used to bound rg's stderr in the
// audit gate against multi-MB dump overflow); it lives here, not in
// `crush_diagnostics`, so the canonical crate stays minimal. If a
// future binary also needs this cap, promote it to the peer crate
// with the same signature.
//
// IMPORTANT: `hinted_text` returns RAW text (no JSON quoting, no
// escaping). The downstream `diag_line_from` applies a single
// `serde_json::to_string` pass via `DiagRecord`, which encodes the
// raw text into a proper JSON-string value once.
//
// The previous implementation returned a PRE-ENCODED JSON-string
// value (already wrapped in `"…"` with content escaped). That was
// correct when `json_diag_line` spliced verbatim, but broke when the
// migration replaced `json_diag_line` with the canonical
// `diag_line_from` (which re-encodes via serde). The result was a
// double-encoded NDJSON record (`"\"...truncated...\""`) — silent
// wire-shape break.
//
// `HINT_MAX_BYTES` is the cap on the JSON-encoded byte length of a
// single `hint` field. 4 KiB is well above human-readable warning
// text (often < 2 KiB) while bounding the worst case to a single
// NDJSON record's worth of bytes.
// =========================================================================

/// Cap on the JSON-encoded byte length of a `hint` field passed to
/// [`diag_line_from`].
pub const HINT_MAX_BYTES: usize = 4096;

/// Encode `text` for use as the `hint` field of a [`DiagRecord`], and
/// cap the **encoded** byte length at [`HINT_MAX_BYTES`].
///
/// Returns RAW text (no surrounding quotes, no JSON escaping) so
/// [`diag_line_from`] can apply a single `serde_json::to_string`
/// pass via the downstream `DiagRecord`'s serde-derived `Serialize`
/// impl. The previous shapes ("pre-encode to JSON-string value")
/// double-encoded under the new path and have been abandoned.
///
/// # Algorithm
///
/// - **Fast path** (`serde::to_string(text).len() ≤ HINT_MAX_BYTES`):
///   return the input as a raw `String`; the downstream serde pass
///   produces a wire-format byte-for-byte equivalent to any other
///   hint that didn't need capping.
///
/// - **Slow path** (encoded form exceeds cap):
///   1. Compute the upper bound on the appended marker's encoded
///      length. The marker template `"... [{N} more bytes truncated]"`
///      is ASCII printable (digits, brackets, dots, spaces,
///      letters) so serde encodes it with zero escaping —
///      `encoded_marker_size = marker_raw.len() + 2` (open + close
///      quotes). The size of the trailing `{N}` placeholder is
///      bounded by the number of digits in `text.len()` (≤ 20 for
///      `usize::MAX`).
///   2. Compute `inner_encoded_budget = HINT_MAX_BYTES − 2 −
///      encoded_marker_upper`. The `−2` accounts for the OUTER quotes
///      around the entire encoded hint string.
///   3. Truncate the raw input to at most `inner_encoded_budget / 6`
///      bytes (on a Unicode codepoint boundary). The `/ 6` is the
///      **worst-case** serde expansion ratio: every RFC 8259 §7
///      escape (`\uNNNN`) is exactly 6 bytes for a 1-byte UTF-8
///      source character (control chars `\x00..\x1F`). Other code
///      points serialize at ≤ their UTF-8 raw-byte count (ASCII
///      printable is 1:1; multi-byte UTF-8 is 1:1 or smaller). So
///      integer-dividing by 6 guarantees the encoded form fits.
///   4. Append `"... [{bytes_truncated} more bytes truncated]"` to
///      the truncated raw prefix (plain chars, not JSON-encoded).
///   5. `debug_assert!` the actual encoded byte count is within
///      `HINT_MAX_BYTES` as a release-build safety net.
///
/// Tradeoff: the prefix is conservative for text dominated by
/// control characters; for ASCII printable text (the common case for
/// human-readable warnings), roughly `HINT_MAX_BYTES − 50` raw
/// characters survive, which matches the "first ~4 KB" framing the
/// comment block above promises.
pub fn hinted_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    // Fast path: serialize-once check. `serde_json::to_string` on a
    // plain `String` is a never-fails operation (only fails on
    // maps-with-non-string-keys, enums with no Serialize impl, etc.;
    // `String` has a built-in Serialize impl that never fails).
    if let Ok(encoded_full) = serde_json::to_string(text) {
        if encoded_full.len() <= HINT_MAX_BYTES {
            // Byte-for-byte equivalence: downstream `serde_json::to_string`
            // on this `String` produces the same bytes as the encoded_full
            // check above (no escaping on a copy of the original text
            // would have changed anything). Return the raw input as-is.
            return text.to_string();
        }
    }
    // Slow path: prefix-truncate on encoded-length budget.
    //
    // Worst-case marker raw length: "... [N more bytes truncated]"
    // where N = text.len() digits. At most 20 digits for u64::MAX.
    // The marker's chars are all ASCII printable (digits 0-9,
    // letters a-z, brackets `[`/`]`, dots `.`, spaces `' '`) — NONE
    // require JSON escaping, so encoded marker size = raw marker
    // size + 2 (open + close quotes wrapping the marker string).
    let marker_raw_upper_len = format!("... [{} more bytes truncated]", text.len()).len();
    let encoded_marker_upper = marker_raw_upper_len + 2;
    // Inner encoded budget = HINT_MAX_BYTES minus the outer quotes (2)
    // minus the encoded marker overhead.
    let inner_encoded_budget = HINT_MAX_BYTES - 2 - encoded_marker_upper;
    // Worst-case serde expansion: each char < 0x20 becomes
    // `\uNNNN` = 6 bytes. Anything else (`>= 0x20` ASCII printable,
    // BMP, multi-byte UTF-8) serializes at ≤ its raw UTF-8 byte
    // count (1:1 or smaller). So 6x is the hard upper bound on
    // encoded-byte / raw-byte ratio, and int-dividing the budget by
    // 6 yields a guaranteed-fit raw-byte cap.
    let prefix_raw_cap = inner_encoded_budget / 6;
    // Walk the input and stop at the largest char boundary whose
    // start byte-offset is less than `prefix_raw_cap`. The
    // `prefix_raw_cap` was chosen as an UPPER BOUND on the raw
    // prefix; if the input is shorter we naturally exit earlier.
    let mut prefix_end_byte: usize = 0;
    for (byte_offset, ch) in text.char_indices() {
        if byte_offset >= prefix_raw_cap {
            break;
        }
        prefix_end_byte = byte_offset + ch.len_utf8();
    }
    let prefix = &text[..prefix_end_byte];
    let bytes_truncated = text.len() - prefix.len();
    let mut out = String::with_capacity(prefix.len() + marker_raw_upper_len);
    out.push_str(prefix);
    out.push_str(&format!("... [{} more bytes truncated]", bytes_truncated));
    // Release-build safety net: serialize-once should fit. If this
    // fires in CI the cap math above is wrong and the wire-format
    // lockdown in `crush-diagnostics/tests/wire_format.rs` won't
    // help because the SDK-side lockdown tests don't exercise the
    // cap path.
    debug_assert!(
        serde_json::to_string(&out)
            .map(|s| s.len() <= HINT_MAX_BYTES)
            .unwrap_or(false),
        "hinted_text exceeded HINT_MAX_BYTES cap: encoded {} cap {} (input len {})",
        serde_json::to_string(&out).map(|s| s.len()).unwrap_or(0),
        HINT_MAX_BYTES,
        text.len(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------------
    // hinted_text — 4 KiB cap (xtask-private surface; canonical
    // wire-format lockdown lives in
    // `crush-diagnostics/tests/wire_format.rs`). The tests below pin
    // the post-migration contract: `hinted_text` returns RAW text and
    // the downstream serde encoder produces a wire-format byte-exact
    // equivalent of what the input would have serialized to without
    // the cap. The previous contract (pre-encoded JSON-string value)
    // double-encoded under the new path and is OBSOLETE.
    // ----------------------------------------------------------------

    #[test]
    fn hinted_text_short_passes_through_verbatim() {
        // RAW passthrough: no surrounding quotes, no escaping.
        // The downstream serde pass is what adds the quotes.
        assert_eq!(hinted_text("hello"), "hello");
        assert_eq!(hinted_text(""), "");
        // Embedded quote comes through LITERALLY (not as `\"`); the
        // downstream serde pass is what escapes it once on the wire.
        assert_eq!(hinted_text("with \"quote\""), "with \"quote\"");
        // A nearly-cap-sized ASCII string passes through unchanged
        // (well under the 6x expansion ceiling).
        let raw = "x".repeat(HINT_MAX_BYTES - 50);
        assert_eq!(hinted_text(&raw), raw);
    }

    #[test]
    fn hinted_text_serde_round_trips_raw_input() {
        // The contract: `serde_json::from_str(serde_json::to_string(
        // hinted_text(text))) == text`. This is the test that catches
        // double-encoding (escaped-inner-quotes inside outer quotes,
        // which serde v1.0+ would produce from a pre-encoded input).
        for text in [
            "hello",
            "",
            "with \"quote\" inside",
            "line\nbreak",
            "back\\slash",
            "tab\there",
            "\u{0001}",
            "☃",
            "\r",
            "🌲",
        ] {
            let raw = hinted_text(text);
            // Single serde pass on the raw text → decodes to the
            // original (no double-encoding, no escaping inside the
            // raw output).
            let encoded = serde_json::to_string(&raw).expect("to_string always");
            let decoded: String =
                serde_json::from_str(&encoded).expect("hinted_text output must be valid JSON");
            assert_eq!(decoded, text, "round trip broke for {text:?}");
        }
    }

    #[test]
    fn hinted_text_truncates_long_with_marker() {
        let big = "x".repeat(HINT_MAX_BYTES * 2);
        let out = hinted_text(&big);
        // Encoded form fits in the cap (the /-shrunken prefix + marker).
        let encoded =
            serde_json::to_string(&out).expect("hinted_text output is always safe to encode");
        assert!(
            encoded.len() <= HINT_MAX_BYTES,
            "encoded byte count must cap at HINT_MAX_BYTES (got {})",
            encoded.len()
        );
        // Marker survives (as raw text in the new contract).
        assert!(out.contains(" more bytes truncated]"));
        assert!(out.contains("...") && out.contains("["));
        // Decode reveals: prefix preserved, marker is part of the value.
        let decoded: String = serde_json::from_str(&encoded).unwrap();
        assert!(decoded.contains(" more bytes truncated]"));
        assert!(decoded.starts_with("x"));
    }

    #[test]
    fn hinted_text_codepoint_boundary_safe() {
        // 4-byte UTF-8 emoji ('🌲' is U+1F332, 4 bytes raw).
        let emoji = "🌲".repeat(HINT_MAX_BYTES);
        let out = hinted_text(&emoji);
        let encoded = serde_json::to_string(&out).expect("encodable");
        assert!(
            encoded.len() <= HINT_MAX_BYTES,
            "encoded byte count cap (got {})",
            encoded.len()
        );
        // Single serde pass: output is a JSON array-of-emoji string.
        let value: serde_json::Value = serde_json::from_str(&encoded).expect("valid JSON");
        let s = value.as_str().expect("output must be a JSON string");
        // Marker appears AND at least one emoji survived.
        assert!(s.chars().any(|c| c == '🌲'));
        assert!(s.contains("more bytes truncated"));
    }

    #[test]
    fn hinted_text_marker_byte_count_is_original_input_bytes() {
        // The marker reports the RAW (UTF-8 byte count) values that
        // didn't fit, NOT the encoded-byte count. A consumer who
        // needs to know "did we throw away a lot" reads the marker
        // N directly.
        let input_len: usize = HINT_MAX_BYTES + 5000;
        let big = "x".repeat(input_len);
        let out = hinted_text(&big);
        let encoded = serde_json::to_string(&out).unwrap();
        let decoded: String = serde_json::from_str(&encoded).unwrap();
        let between = decoded
            .split("... [")
            .nth(1)
            .and_then(|s| s.split(" more bytes truncated]").next())
            .expect("marker should be present when truncation occurs");
        let n: usize = between
            .parse()
            .expect("marker N should parse as unsigned decimal");
        assert!(n > 0, "marker byte count must be positive (got {n})");
        assert!(
            n < input_len,
            "marker byte count must be < input length (got {n}, input_len={input_len})"
        );
    }

    #[test]
    fn hinted_text_with_special_char_at_boundary() {
        // Control chars (< 0x20) trigger the 6x worst-case
        // expansion path. The cap math MUST handle them; this test
        // is a smoke that no half-encoded escape sequence ever
        // surfaces (the pre-encoding era had a bug class here).
        let ctrl = "\u{0001}".repeat(HINT_MAX_BYTES);
        let out = hinted_text(&ctrl);
        let encoded = serde_json::to_string(&out).expect("encodable");
        assert!(encoded.len() <= HINT_MAX_BYTES, "cap must hold on ctrl chars");
        let _: serde_json::Value = serde_json::from_str(&encoded)
            .expect("no half-encoded \\uNNNN escape may appear in output");
    }

    // ----------------------------------------------------------------
    // import-smoke — confirm the canonical re-exports resolve cleanly.
    // Full wire-format lockdown is in
    // `crush-diagnostics/tests/wire_format.rs`.
    // ----------------------------------------------------------------

    #[test]
    fn diag_import_resolves() {
        assert_eq!(CODE_AUDIT, "E-AUDIT");
        assert_eq!(CODE_LINT, "E-LINT");
        assert_eq!(CODE_IO, "E-IO");
        assert!(wants_json(&[
            "xtask".to_string(),
            "audit".to_string(),
            "--message-format=json".to_string(),
        ]));
        assert!(!wants_json(&["xtask".to_string(), "audit".to_string()]));
    }
}
