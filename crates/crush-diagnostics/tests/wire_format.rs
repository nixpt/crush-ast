//! Canonical NDJSON wire-format lockdown for the entire Crush CLI
//! surface.
//!
//! Every wireable binary (`xtask`, `crush-vm`, `crush-installer`,
//! `crush-pkg`) routes through this crate's [`DiagRecord`] /
//! [`diag_line`] / [`diag_line_from`] / [`wants_json`] /
//! [`strict_downgrade`]. The tests in this file are the single
//! source of truth for the seven-field wire shape AND the
//! per-binary message-format dispatch semantics; per-binary
//! lockdown tests are gone (replaced by the call into the
//! canonical surface).
//!
//! Pair with [`crush_lang_sdk/src/theme.rs::JsonDiagnostic`] tests:
//! that file locks the canonical SDK struct's wire shape; this file
//! locks the cross-binary shape produced by tools that DO NOT
//! depend on `crush-lang-sdk` (the four quarantined binaries
//! referenced above). Together they cover the entire CLI surface.

use std::borrow::Cow;

use crush_diagnostics::{
    BorrowedDiagRecord, DiagRecord, OwnedDiagRecord, consume_stream, consume_stream_borrowed,
    diag_line, diag_line_from, parse_record, parse_record_borrowed, strict_downgrade,
    wants_json,
};

// ----------------------------------------------------------------
// diag_line — byte-exact field order
// ----------------------------------------------------------------

#[test]
fn diag_line_field_order_is_canonical() {
    // Pin the seven-field wire shape so a contributor who reorders
    // `DiagRecord`'s field-DECLARATION order breaks this test
    // (and the canonically-locked test in
    // `crush_lang_sdk/src/theme.rs::tests`). Mirrors the byte-exact
    // output previously pinned in `xtask::diag::tests`,
    // `crush-vm::tests`, `crush-installer::tests`, and
    // `crush-pkg::tests`.
    let rec = DiagRecord {
        code: "E-AUDIT",
        level: "error",
        file: None,
        line: None,
        col: None,
        message: "site boundary",
        hint: None,
    };
    let actual = diag_line(&rec);
    let expected = "{\"code\":\"E-AUDIT\",\"level\":\"error\",\"file\":null,\"line\":null,\"col\":null,\"message\":\"site boundary\",\"hint\":null}\n";
    assert_eq!(actual, expected);
}

#[test]
fn diag_line_with_file_and_hint_serializes() {
    // Previously pinned in `xtask::diag::tests::json_diag_line_with_file_and_hint_serializes`.
    let rec = DiagRecord {
        code: "E-LINT",
        level: "error",
        file: Some(".dejavue/timeline.jsonl"),
        line: None,
        col: None,
        message: "candidate_form=foo | bar",
        hint: Some("install ripgrep instead of grep"),
    };
    let actual = diag_line(&rec);
    let expected = "{\"code\":\"E-LINT\",\"level\":\"error\",\"file\":\".dejavue/timeline.jsonl\",\"line\":null,\"col\":null,\"message\":\"candidate_form=foo | bar\",\"hint\":\"install ripgrep instead of grep\"}\n";
    assert_eq!(actual, expected);
}

#[test]
fn diag_line_with_hint_serializes_vm_style() {
    // Previously pinned in `crush-vm::tests::json_diag_line_with_hint_serializes`.
    let rec = DiagRecord {
        code: "E-ASM",
        level: "error",
        file: Some("f.casm"),
        line: None,
        col: None,
        message: "line 3: bad opcode",
        hint: Some("see https://example.com/docs/casm"),
    };
    let actual = diag_line(&rec);
    let expected = "{\"code\":\"E-ASM\",\"level\":\"error\",\"file\":\"f.casm\",\"line\":null,\"col\":null,\"message\":\"line 3: bad opcode\",\"hint\":\"see https://example.com/docs/casm\"}\n";
    assert_eq!(actual, expected);
}

#[test]
fn diag_line_no_file_or_hint_defaults_to_null() {
    // Previously pinned in `crush-installer::tests::diag_record_no_file_or_hint_defaults_to_null`.
    let rec = DiagRecord {
        code: "E-INSTALL",
        level: "error",
        file: None,
        line: None,
        col: None,
        message: "x",
        hint: None,
    };
    let s = diag_line(&rec);
    assert!(s.contains("\"file\":null"));
    assert!(s.contains("\"hint\":null"));
    assert!(s.contains("\"line\":null"));
    assert!(s.contains("\"col\":null"));
}

#[test]
fn diag_line_message_with_embedded_quotes_round_trip() {
    // Previously pinned across 4 binaries; round-trip via serde_json
    // so any encoding break (quote mishandling, control-char
    // mishandling) surfaces synchronously rather than at the editor
    // consumer's parse step.
    let rec = DiagRecord {
        code: "E-AUDIT",
        level: "error",
        file: None,
        line: None,
        col: None,
        message: "site: file=src/x.rs:42 match=\"Value::Handle(MARKER)\"",
        hint: None,
    };
    let value: serde_json::Value = serde_json::from_str(diag_line(&rec).trim_end()).unwrap();
    assert_eq!(value["code"], "E-AUDIT");
    assert_eq!(value["level"], "error");
    assert_eq!(
        value["message"],
        "site: file=src/x.rs:42 match=\"Value::Handle(MARKER)\""
    );
    assert!(value["hint"].is_null());
    assert!(value["file"].is_null());
    assert!(value["line"].is_null());
    assert!(value["col"].is_null());
}

#[test]
fn diag_line_serialize_fields_have_canonical_order() {
    // Belt-and-suspenders against re-orderings that preserve
    // serde_json::Value semantics: streaming-JSON consumers that
    // care about per-token ordering for grammar-validity budgets
    // MUST see fields in the canonical declaration order.
    let rec = DiagRecord {
        code: "E-AUDIT",
        level: "error",
        file: Some("/x"),
        line: Some(42),
        col: Some(7),
        message: "x",
        hint: Some("y"),
    };
    let s = diag_line(&rec);
    let c = s.find("\"code\":").unwrap();
    let l = s.find("\"level\":").unwrap();
    let f = s.find("\"file\":").unwrap();
    let li = s.find("\"line\":").unwrap();
    let co = s.find("\"col\":").unwrap();
    let m = s.find("\"message\":").unwrap();
    let h = s.find("\"hint\":").unwrap();
    assert!(c < l && l < f && f < li && li < co && co < m && m < h);
}

// ----------------------------------------------------------------
// diag_line_from byte-equals diag_line (function vs struct parity)
// ----------------------------------------------------------------

#[test]
fn diag_line_from_byte_equals_diag_line() {
    // Function-form shortcut is byte-equivalent to the struct-form
    // canonical path FOR THE NO-LINE/NO-COL SHORTHAND (the
    // documented equivalence — see [`diag_line_from`]'s docstring,
    // which hardcodes `line: None, col: None`). Both paths are part
    // of the public API; this pins the equivalence for that exact
    // shape so a future refactor that optimizes the function path
    // breaks this test synchronously.
    //
    // Test cases with `Some(line) / Some(col)` go through the
    // struct-form path (`diag_line(&DiagRecord { ... })`) since
    // `diag_line_from` deliberately elides line/col (the most
    // common site is short call sites where attaching a line/col
    // would be over-ceremony).
    let from_struct = diag_line(&DiagRecord {
        code: "E-PKG-MANIFEST",
        level: "error",
        file: Some("capsule.toml"),
        line: None,
        col: None,
        message: "manifest decode failed",
        hint: None,
    });
    let from_fn = diag_line_from(
        "E-PKG-MANIFEST",
        "error",
        "manifest decode failed",
        None,
        Some("capsule.toml"),
    );
    assert_eq!(from_struct, from_fn);
    assert!(
        from_struct.contains(r#""line":null"#) && from_struct.contains(r#""col":null"#),
        "diag_line_from shorthand must produce line=null, col=null (got: {from_struct:?})"
    );
}

// ----------------------------------------------------------------
// DiagRecord — coarser-than-exact sanity that the canonical shape
// still accepts the per-binary wire codes used across the migrated
// quarantine sites. Pinning the *exact* codes lives in each binary
// (it varies per domain), but the canonical surface MUST be able to
// carry arbitrary `&'a str` codes — the test below re-exercises
// every code the four quarantined binaries carry across this peer
// crate, and asserts the canonical `diag_line_from` produces the
// same byte shape regardless of which code is used. (If a future
// refactor restricts `code` to a fixed enum, this test will catch
// the divergence synchronously.)
// ----------------------------------------------------------------

#[test]
fn diag_record_accepts_all_per_binary_wire_codes() {
    // Every code string the four quarantined binaries use across
    // this peer crate — `xtask` (audit / lint / io),
    // `crush-installer`, `crush-pkg` (per-domain), and the
    // historical `crush-vm` codes. Co-locating the literal list in
    // one test means a refactor that adds/renames a code only
    // requires updating THIS list, not every binary's tests.
    let codes = [
        "E-AUDIT",       // xtask audit
        "E-LINT",        // xtask lint-dejavue
        "E-IO",          // xtask lint-dejavue / generic I/O
        "E-INSTALL",     // crush-installer
        "E-NEW",         // crush-pkg new
        "E-MANIFEST",    // crush-pkg pack/unpack/show
        "E-BUILDER",     // crush-pkg build/check
        "E-RUN",         // crush-pkg run
        "E-SIGN",        // crush-pkg sign/verify/keygen
        "E-SITE",        // crush-pkg site/site-extract
        "E-ASM",         // crush-vm (kept around for parity)
        "E-VM-INTERNAL", // crush-vm (kept around for parity)
    ];
    for code in codes {
        // sanity: any code delimiter convention (single dash after
        // E prefix is the canonical form). The peer crate doesn't
        // enforce the form — this test is for human-readability
        // only, so a refactor that adds a 3-letter prefix code
        // doesn't fail on the convention but still passes the form
        // assertion by virtue of starting with "E-".
        assert!(
            code.starts_with("E-"),
            "per-binary wire codes should follow E-NAME convention (got {code:?})"
        );
        // canonical surface accepts the code and produces the
        // expected field position (code is the FIRST field).
        let line = diag_line_from(code, "error", "msg", None, None);
        let expected_prefix = format!(r#"{{"code":"{code}","level":"error","file":null,"line":null,"col":null,"message":"msg","hint":null"#);
        assert!(
            line.starts_with(&expected_prefix),
            "diag_line_from must accept arbitrary per-binary code (got: {line:?})"
        );
    }
}

// ----------------------------------------------------------------
// wants_json — flag parsing (consolidated xtask + crush-vm cases)
// ----------------------------------------------------------------

#[test]
fn wants_json_flags() {
    let cases: Vec<(Vec<String>, bool)> = vec![
        // Default text mode.
        (vec!["xtask".into(), "audit".into()], false),
        (vec!["crush-vm".into(), "run".into()], false),
        // All three flag forms accepted.
        (
            vec![
                "xtask".into(),
                "audit".into(),
                "--message-format=json".into(),
            ],
            true,
        ),
        (
            vec![
                "xtask".into(),
                "audit".into(),
                "--message-format".into(),
                "json".into(),
            ],
            true,
        ),
        (
            vec![
                "xtask".into(),
                "audit".into(),
                "--message-format-json".into(),
            ],
            true,
        ),
        // Non-json formats rejected.
        (
            vec![
                "xtask".into(),
                "audit".into(),
                "--message-format=text".into(),
            ],
            false,
        ),
        // Positional flexibility: flag before OR after subcommand.
        (
            vec![
                "--message-format=json".into(),
                "xtask".into(),
                "audit".into(),
            ],
            true,
        ),
        (
            vec![
                "crush-vm".into(),
                "--message-format=json".into(),
                "run".into(),
            ],
            true,
        ),
        // Multi-argv case (lint-dejavue doesn't take "audit" subcommand).
        (
            vec!["lint-dejavue".into(), "--message-format=json".into()],
            true,
        ),
        (
            vec![
                "lint-dejavue".into(),
                "/tmp/timeline.jsonl".into(),
                "--message-format".into(),
                "json".into(),
            ],
            true,
        ),
        // Double-flag (should still resolve true).
        (
            vec![
                "crush-vm".into(),
                "--message-format=json".into(),
                "--message-format-json".into(),
                "run".into(),
            ],
            true,
        ),
    ];
    for (args, want) in cases {
        assert_eq!(
            wants_json(&args),
            want,
            "wants_json({args:?}) should be {want}"
        );
    }
}

// ----------------------------------------------------------------
// strict_downgrade — strict-mode level-promotion kernel
//
// Hoisted from `crush-pkg::main` (private) to canonical so any
// future binary adopting `--message-format=strict` routes
// through one canonical implementation. The kernel is small
// enough that the lockdown tests are comprehensive rather than
// matrix-driven; coverage axis is:
//
//   A. Passthrough matrix — non-`note` levels pass through
//      unchanged under both strict and non-strict mode.
//   B. Non-strict symmetry — `note` under non-strict is also a
//      passthrough (so strict is the ONLY trigger).
//   C. Strict-mode kernel — `note` under strict lifts to `error`.
//      This is the only mutation the kernel performs.
//   D. Return-`&str` shape — passthrough arm returns the input
//      `level` reborrowed (same memory); lift arm returns the
//      canonical `"error"` static literal. Zero-alloc evidence
//      via pointer-equality on both arms.
// ----------------------------------------------------------------

#[test]
fn strict_downgrade_passes_through_non_note_levels_under_both_modes() {
    // Non-`note` levels are unaffected by `strict_mode`. Confirms
    // the kernel is scoped (only `note` lifts), so a contributor
    // who mistakenly widened the rule to also elevate `warning`
    // or `info` would fail this test synchronously.
    let levels = ["error", "warning", "info"];
    for level in levels {
        assert_eq!(
            strict_downgrade(level, /* strict_mode */ true),
            level,
            "non-note level must pass through under strict (level={level:?})"
        );
        assert_eq!(
            strict_downgrade(level, /* strict_mode */ false),
            level,
            "non-note level must pass through under non-strict (level={level:?})"
        );
    }
}

#[test]
fn strict_downgrade_note_is_passthrough_under_non_strict() {
    // `note` is the ONLY class sensitive to `strict_mode`. Pairs
    // with the lift test below so the matrix `(level in note,
    // strict in {true, false})` has its full diagonal pinned.
    assert_eq!(
        strict_downgrade("note", /* strict_mode */ false),
        "note",
        "note under non-strict must remain \"note\""
    );
}

#[test]
fn strict_downgrade_note_lifts_to_error_under_strict() {
    // The CI-gate kernel: the ONLY mutation the helper performs.
    // If a future contributor narrows the rule (e.g. only lifts
    // on a specific level) or widens it (also lifts on
    // `warning`), this test fails synchronously. Pair with
    // `strict_downgrade_passes_through_non_note_levels...` above
    // so both `note` arms of the matrix are pinned.
    assert_eq!(
        strict_downgrade("note", /* strict_mode */ true),
        "error",
        "note under strict MUST lift to error (CI gate)"
    );
}

// ----------------------------------------------------------------
// wire_consumer — round-trip parity between the emitter (`diag_line`)
// and the canonical parser (`parse_record`). The deserialize arm of
// the wire contract lives in `src/wire_consumer.rs` (with its own
// in-crate tests); the test below locks the round-trip in THIS file
// so the byte-exact wire shape — both directions — is pinned in the
// canonical lockdown file, matching this crate's stated philosophy.
// ----------------------------------------------------------------

#[test]
fn parse_record_roundtrips_diag_line_emitted_canonical_note_record() {
    // The bidirectional wire contract: a `DiagRecord` emitted by
    // `diag_line` MUST parse back into an equivalent `OwnedDiagRecord`.
    // If the emitter or parser field order drifts, this breaks.
    let rec = DiagRecord {
        code: "E-BUILDER",
        level: "note",
        file: Some("capsule.toml"),
        line: Some(7),
        col: None,
        message: "placeholder value `TEMP` must be filled in",
        hint: Some("set TEMP in your shell before running"),
    };
    let text = diag_line(&rec);
    let parsed = parse_record(&text).expect("must parse a canonical emitter line");
    let expected = OwnedDiagRecord::from(&rec);
    assert_eq!(parsed, expected);
}

#[test]
fn consume_stream_roundtrips_multiple_diag_lines() {
    // Stream-level parity: two emitted lines, separated by a blank
    // line (common in piped CI output), both round-trip via
    // `consume_stream`.
    let rec1 = DiagRecord {
        code: "E-LINT",
        level: "error",
        file: None,
        line: None,
        col: None,
        message: "first",
        hint: None,
    };
    let rec2 = DiagRecord {
        code: "E-LINT",
        level: "error",
        file: Some("x.crush"),
        line: Some(1),
        col: Some(2),
        message: "second",
        hint: None,
    };
    let stream = format!(
        "\n{}\n\n{}\n",
        diag_line(&rec1).trim_end(),
        diag_line(&rec2).trim_end(),
    );
    let records: Vec<OwnedDiagRecord> = consume_stream(stream.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .expect("all non-blank lines must parse");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0], OwnedDiagRecord::from(&rec1));
    assert_eq!(records[1], OwnedDiagRecord::from(&rec2));
}

// ----------------------------------------------------------------
// BorrowedDiagRecord — zero-copy deserialization lockdown
//
// Pins the zero-copy contract: unescaped JSON strings MUST borrow
// directly from the input (Cow::Borrowed, pointer equality with the
// input buffer) on the REQUIRED fields (code, level, message).
// Escaped strings fall back to Cow::Owned (allocation, but no error).
// The OPTIONAL fields (file, hint) are wrapped in Option<Cow> and
// serde_json does not propagate borrows through Option — those always
// allocate. This is a known serde_json limitation documented on
// BorrowedDiagRecord.
// ----------------------------------------------------------------

#[test]
fn parse_record_borrowed_zero_copy_for_unescaped_strings() {
    let line = r#"{"code":"E-LINT","level":"error","file":"src/x.rs","line":42,"col":7,"message":"site boundary","hint":"fix it"}"#;
    let rec = parse_record_borrowed(line).expect("unescaped line must parse");

    // Required fields: must be Borrowed (zero-copy).
    assert!(matches!(rec.code, Cow::Borrowed(_)), "code must be Borrowed");
    assert!(matches!(rec.level, Cow::Borrowed(_)), "level must be Borrowed");
    assert!(matches!(rec.message, Cow::Borrowed(_)), "message must be Borrowed");

    // Optional fields: serde_json doesn't propagate borrows through
    // Option<Cow>, so these are Owned (allocated). Values are correct.
    assert_eq!(rec.file.as_deref(), Some("src/x.rs"));
    assert_eq!(rec.hint.as_deref(), Some("fix it"));

    // Pointer equality: the borrowed slice must point into `line`.
    let code_ptr = rec.code.as_ptr();
    let line_code_start = line.find("\"E-LINT\"").unwrap() + 1;
    assert_eq!(
        code_ptr, line[line_code_start..].as_ptr(),
        "code must point into the input buffer (zero-copy)"
    );
}

#[test]
fn parse_record_borrowed_falls_back_to_owned_for_escaped_strings() {
    // message contains an escaped quote: "has \"quote\""
    let line = r#"{"code":"E-LINT","level":"error","file":null,"line":null,"col":null,"message":"has \"quote\"","hint":null}"#;
    let rec = parse_record_borrowed(line).expect("escaped line must parse (not error)");

    assert!(matches!(rec.code, Cow::Borrowed(_)));
    assert!(matches!(rec.level, Cow::Borrowed(_)));
    assert!(
        matches!(rec.message, Cow::Owned(_)),
        "message with escapes must be Owned (allocated), not Borrowed"
    );
    assert_eq!(rec.message, "has \"quote\"");
}

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
    let text = diag_line(&rec);
    let parsed = parse_record_borrowed(&text).expect("must parse a canonical emitter line");
    assert_eq!(parsed.code, rec.code);
    assert_eq!(parsed.level, rec.level);
    assert_eq!(parsed.file.as_deref(), Some("capsule.toml"));
    assert_eq!(parsed.line, Some(7));
    assert_eq!(parsed.col, None);
    assert_eq!(parsed.message, rec.message);
    assert_eq!(parsed.hint.as_deref(), Some("set TEMP in your shell before running"));
}

#[test]
fn consume_stream_borrowed_skips_blanks_and_yields_records() {
    let rec1 = DiagRecord {
        code: "E-LINT", level: "error", file: None, line: None, col: None,
        message: "first", hint: None,
    };
    let rec2 = DiagRecord {
        code: "E-LINT", level: "error", file: Some("x.crush"), line: Some(1), col: Some(2),
        message: "second", hint: None,
    };
    let stream = format!(
        "\n{}\n\n{}\n",
        diag_line(&rec1).trim_end(),
        diag_line(&rec2).trim_end(),
    );
    let records: Vec<BorrowedDiagRecord<'_>> = consume_stream_borrowed(stream.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .expect("all non-blank lines must parse");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].code, "E-LINT");
    assert_eq!(records[0].message, "first");
    assert_eq!(records[1].file.as_deref(), Some("x.crush"));
    assert_eq!(records[1].line, Some(1));
    assert_eq!(records[1].col, Some(2));
}

#[test]
fn strict_downgrade_returns_borrowed_str_zero_alloc() {
    // Both arms are borrowed: passthrough re-borrows the input;
    // lift returns the canonical `"error"` static. Pin via
    // pointer-equality so a future contributor who accidentally
    // returns an owned `String` (allocation on the passthrough
    // arm) fails this test synchronously.
    //
    // Lifted-arm check: two calls return the same `&str` with
    // identical pointer — only possible if both resolve to the
    // canonical static `"error"` literal.
    let a = strict_downgrade("note", true);
    let b = strict_downgrade("note", true);
    assert_eq!(
        a.as_ptr(),
        b.as_ptr(),
        "lift arm must return the same canonical static literal (zero-alloc)"
    );
    assert_eq!(a, "error");
    assert_eq!(b, "error");
    // Passthrough-arm check: the returned `&str` borrows from the
    // input `level` arg, so the pointer MUST be the same as the
    // input slice's first byte. A future alloc-on-passthrough
    // refactor would break this pointer-equality.
    let input = String::from("warning");
    let r = strict_downgrade(&input, true);
    assert_eq!(
        r.as_ptr(),
        input.as_ptr(),
        "passthrough arm must return a re-borrow of the input (zero-alloc)"
    );
}
