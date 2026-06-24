//! End-to-end integration test for `crush_index::stale` + `codebase.*` caps.
//!
//! Companion to (and lock for) the direct-Program tests in
//! `crush_lang_sdk::codebase::tests::*`. Those tests cover the cap
//! layer in isolation (a hand-built `Program.temporaries = vec![...]`
//! seed). **This file covers the SOURCE pipeline**: real Crush source
//! → `Parser::parse` → `CrushIndex::add_program` → `register_at` with
//! a pinned `today` → `Runtime::with_host_caps` + CASM cap call →
//! `io.print` → output-string assertion.
//!
//! Going through the parser + indexing pipeline catches regressions
//! in three independent places at once — the parser stops
//! recognising `@temporary { reason: ..., added: "..." }`, the
//! indexer drops the field, or the cap → `io.print` stringification
//! silently changes shape — any one of which the direct-Program
//! tests cannot detect on their own.
//!
//! ## Wall-clock independence
//!
//! Both tests pin `today = 2026-06-20` via `register_at`, so the
//! 90-day boundary asserts reproduce deterministically regardless
//! of `chrono::Utc::now()` at test time. Production callers default
//! `today` to wall-clock via `register(caps, index)`; tests that
//! exercise boundary math must use the `*_at` test seam directly.

use chrono::NaiveDate;
use crush_cast::Program;
use crush_frontend::parser::Parser;
use crush_index::CrushIndex;
use crush_lang_sdk::codebase::register_at;
use crush_lang_sdk::{HostCaps, ProgramBuilder, Runtime};
use std::sync::Arc;

/// Pinned "today" so the 90-day boundary math is wall-clock-independent:
/// ```text
///   2026-06-19 → 1 day back     (fresh)
///   2026-03-22 → exactly 90 days back (fresh per strict-<)
///   2026-03-21 → 91 days back   (stale)
/// ```
fn pin_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 20).expect("hard-coded test date is valid")
}

/// Real Crush source containing four `@temporary` blocks exercising
/// distinct boundary cases. The single trailing `fn f() { }` keeps the
/// parser happy (modules with no function body are rejected).
const E2E_SOURCE: &str = "\
@module {
    purpose: \"e2e stale-temporaries test fixture\"
    exports: [f]
}

@temporary {
    reason: \"fresh canary\"
    added: \"2026-06-19\"
}
@temporary {
    reason: \"boundary canary\"
    added: \"2026-03-22\"
}
@temporary {
    reason: \"stale canary\"
    added: \"2026-03-21\"
}
@temporary {
    reason: \"no-added canary\"
}

fn f() { }
";

/// Parse the E2E source, build a `CrushIndex`, and register every
/// `codebase.*` cap against a `HostCaps` set pinned to `pin_today`.
fn build_e2e_caps(pin_today: NaiveDate) -> HostCaps {
    let prog: Program = Parser::parse(E2E_SOURCE).expect("parse");
    let mut index = CrushIndex::new();
    index.add_program("e2e_stale_mod", &prog);
    let mut caps = HostCaps::new();
    register_at(&mut caps, Arc::new(index), pin_today);
    caps
}

#[test]
fn e2e_stale_temporaries_filters_via_parsed_source() {
    let caps = build_e2e_caps(pin_today());

    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("codebase.stale_temporaries")
        .line(".func main")
        .line(r#"CAP_CALL "codebase.stale_temporaries" 0"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let result = Runtime::new()
        .with_host_caps(caps)
        .run(&program)
        .expect("run");

    assert!(result.halted, "the CASM program should halt cleanly");

    // Inclusive positive: the stale row must appear.
    assert!(
        result.output.contains("stale canary"),
        "stale row (91 days back) must appear in stale_temporaries():\n{}",
        result.output,
    );

    // Three negatives — guard the filter predicate, not just the data.
    // A broken predicate could spuriously include all four rows;
    // a broken parser/indexer could spuriously include none.
    assert!(
        !result.output.contains("fresh canary"),
        "fresh row (1 day back) should be filtered out:\n{}",
        result.output,
    );
    assert!(
        !result.output.contains("boundary canary"),
        "exactly 90-day row should be filtered out (strict-<):\n{}",
        result.output,
    );
    assert!(
        !result.output.contains("no-added canary"),
        "missing-`added` row should be filtered out (silent-skip):\n{}",
        result.output,
    );
}

#[test]
fn e2e_temporaries_lists_all_rows_with_metadata_via_parsed_source() {
    let caps = build_e2e_caps(pin_today());

    let program = ProgramBuilder::new()
        .permission("io.print")
        .permission("codebase.temporaries")
        .line(".func main")
        .line(r#"CAP_CALL "codebase.temporaries" 0"#)
        .line(r#"CAP_CALL "io.print" 1"#)
        .line("HALT")
        .build()
        .expect("build");

    let result = Runtime::new()
        .with_host_caps(caps)
        .run(&program)
        .expect("run");

    assert!(result.halted, "the CASM program should halt cleanly");

    // All four reasons must appear in the unfiltered list — this is
    // what differentiates `temporaries()` (rows_with_metadata) from
    // `stale_temporaries()` (rows_filtered_by_age).
    for reason in ["fresh canary", "boundary canary", "stale canary", "no-added canary"] {
        assert!(
            result.output.contains(reason),
            "row {reason:?} must appear in temporaries() output:\n{}",
            result.output,
        );
    }

    // `is_stale` boolean — fresh/boundary/no-added are FRESH
    // (`is_stale: false`); stale row is STALE (`is_stale: true`).
    assert!(
        result.output.contains("is_stale: false"),
        "fresh/boundary/no-added rows must have is_stale: false:\n{}",
        result.output,
    );
    assert!(
        result.output.contains("is_stale: true"),
        "stale row (91 days back) must have is_stale: true:\n{}",
        result.output,
    );

    // `days_old: 90` and `days_old: 91` are unique substrings of the
    // expected day counts (no "90"/"91" appear in the fixture source
    // or reasons), so any match must come from a `days_old` value.
    // Skipping "1" intentionally: it could also be a substring of "91".
    assert!(
        result.output.contains("days_old: 90"),
        "boundary row (90 days back) must have days_old: 90:\n{}",
        result.output,
    );
    assert!(
        result.output.contains("days_old: 91"),
        "stale row (91 days back) must have days_old: 91:\n{}",
        result.output,
    );

    // `Value::Null` is rendered by `io.print` as the literal three-character
    // sequence `null` (NOT as the empty string `""` that `value_to_string`
    // produces — the E2E test caught a real subtle distinction between
    // the two formatters). The no-added row's `days_old` is `Value::Null`,
    // so the literal substring `days_old: null` must appear in the output.
    // (Other `Value::Null` fields like `expires_when`/`owner`/`added`
    // also render as `null`; the `days_old: null` substring is unique
    // because no other field is named `days_old`.)
    assert!(
        result.output.contains("days_old: null"),
        "no-added row must render days_old as the literal \"null\" (Value::Null):\n{}",
        result.output,
    );
}
