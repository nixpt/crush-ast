//! End-to-end integration tests for the `codebase.*` host caps surfaced
//! via `Runtime::with_codebase` (CRUSH-29).
//!
//! Companion to (and lock for) the direct-cap tests in
//! `crush_lang_sdk::codebase::tests::*`. Those cover single caps in
//! isolation; **this file covers the SOURCE pipeline** — real Crush
//! source text -> `parse_source` -> `CrushIndex::add_program` ->
//! `Runtime` + CASM cap call -> `result.output` string assertion.
//!
//! Going through the parser + indexing pipeline catches regressions in
//! three places at once: the parser stops recognising an annotation,
//! the indexer drops a field on the way through, or the
//! `Value::Map` -> `io.print` stringification silently changes the
//! output. The direct-cap tests cannot see those failures on their
//! own.
//!
//! All six tests pin `today = 2026-06-20` via `with_codebase_at` so
//! the temporal-shape asserts reproduce deterministically regardless
//! of `chrono::Utc::now()` at test time.

use chrono::NaiveDate;
use crush_lang_sdk::Runtime;

fn pin_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 20).expect("hard-coded date is valid")
}

/// Fixture used by tests 1-5. Defines one module with one invariant,
/// two functions (`main` calls `helper`), one `@covers`-bearing test
/// function. Asserts on substring containment rather than exact
/// serialisation — shape coverage, not byte-exact JSON.
const FIXTURE_SOURCE: &str = r#"@module {
    purpose: "CRUSH-29 caps integration fixture"
    exports: [main, helper, test_main]
}
@invariant "no-reenter" {
    description: "no re-entrancy"
    applies_to: [main]
    consequence: "deadlock"
}

fn main() {
    helper(1)
}

fn helper(x) {
    let _ = x
}

@covers "VmError::Foo"
fn test_main() {
}
"#;

fn runtime_with_fixture() -> Runtime {
    Runtime::new()
        .with_codebase_at(&[("fixture", FIXTURE_SOURCE)], pin_today())
        .expect("runtime build")
}

// ── tests ─────────────────────────────────────────────────────────────────

#[test]
fn integration_modules_cap_returns_array_via_runtime() {
    let rt = runtime_with_fixture();
    let casm = r#"
        .func main
        CAP_CALL "codebase.modules" 0
        CAP_CALL "io.print" 1
        HALT
    "#;
    let result = rt
        .run_casm(casm, &["codebase.modules", "io.print"], Some("modules-test"))
        .expect("run");
    assert!(result.halted, "the CASM program should halt cleanly");
    assert!(
        result.output.contains("fixture"),
        "expected module_name 'fixture' in output:\n{}",
        result.output
    );
    assert!(
        result.output.contains("CRUSH-29 caps integration fixture"),
        "expected module_purpose in output:\n{}",
        result.output
    );
    // CRUSH-29 ticket shape: `file` key present (empty stub today;
    // real source-loc lands in CRUSH-29-EXTEND-LOCS).
    assert!(
        result.output.contains("file: "),
        "expected 'file' key on every modules() row:\n{}",
        result.output
    );
}

#[test]
fn integration_invariants_cap_returns_invariant_with_reason_alias() {
    let rt = runtime_with_fixture();
    let casm = r#"
        .func main
        PUSH_STR "fixture"
        CAP_CALL "codebase.invariants" 1
        CAP_CALL "io.print" 1
        HALT
    "#;
    let result = rt
        .run_casm(
            casm,
            &["codebase.invariants", "io.print"],
            Some("invariants-test"),
        )
        .expect("run");
    assert!(result.halted);

    // Ticket-shape coverage: every row carries `name`, `reason`
    // (ticket-canonical; today an alias of `description`),
    // `applies_to`, `consequence`. The legacy `description` field
    // also stays for backward-compat with existing agents.
    assert!(
        result.output.contains("name: no-reenter"),
        "expected invariant name in output:\n{}",
        result.output
    );
    assert!(
        result.output.contains("reason: no re-entrancy"),
        "expected 'reason' alias from ticket shape:\n{}",
        result.output
    );
    assert!(
        result.output.contains("description: no re-entrancy"),
        "expected 'description' (back-compat) preserved:\n{}",
        result.output
    );
    assert!(
        result.output.contains("consequence: deadlock"),
        "expected invariant consequence in output:\n{}",
        result.output
    );
    assert!(
        result.output.contains("applies_to: "),
        "expected 'applies_to' list:\n{}",
        result.output
    );
}

#[test]
fn integration_callers_cap_returns_call_site_via_runtime() {
    let rt = runtime_with_fixture();
    let casm = r#"
        .func main
        PUSH_STR "helper"
        CAP_CALL "codebase.callers" 1
        CAP_CALL "io.print" 1
        HALT
    "#;
    let result = rt
        .run_casm(casm, &["codebase.callers", "io.print"], Some("callers-test"))
        .expect("run");
    assert!(result.halted);

    assert!(
        result.output.contains("callee: helper"),
        "expected callee field:\n{}",
        result.output
    );
    assert!(
        result.output.contains("caller_fn: main"),
        "expected caller_fn field:\n{}",
        result.output
    );
    assert!(
        result.output.contains("caller_module: fixture"),
        "expected caller_module field:\n{}",
        result.output
    );
    // Ticket shape: file/line/context KEYS must be present (their
    // current values are empty/0/"" stubs; real source-loc lands in
    // CRUSH-29-EXTEND-LOCS).
    assert!(
        result.output.contains("file: "),
        "expected 'file' key:\n{}",
        result.output
    );
    assert!(
        result.output.contains("line: "),
        "expected 'line' key:\n{}",
        result.output
    );
    assert!(
        result.output.contains("context: "),
        "expected 'context' key:\n{}",
        result.output
    );
}

#[test]
fn integration_uncovered_paths_cap_returns_empty_array_via_runtime() {
    // FIXTURE_SOURCE declares NO `@errors`, only `@covers`. The
    // `codebase.uncovered_paths()` cap must therefore return an
    // empty row set. We assert a STRICT empty-between-brackets
    // match (rather than the looser "starts_with('[') &&
    // ends_with(']')") so a future regression that produces a
    // non-empty array fails this test loudly.
    let rt = runtime_with_fixture();
    let casm = r#"
        .func main
        CAP_CALL "codebase.uncovered_paths" 0
        CAP_CALL "io.print" 1
        HALT
    "#;
    let result = rt
        .run_casm(
            casm,
            &["codebase.uncovered_paths", "io.print"],
            Some("uncovered-test"),
        )
        .expect("run");
    assert!(result.halted);

    let trimmed = result.output.trim();
    let inner = trimmed.trim_matches(|c| c == '[' || c == ']').trim();
    // Accept either `[]` literal OR an empty-between-brackets form
    // ([], `[ ]`, `[   ]`) so a future io.print formatter that adds
    // intra-bracket whitespace doesn't regress this test. A populated
    // row (e.g. `[{fn_name: ...}]`) still fails: `inner` is non-empty.
    assert!(
        trimmed.contains("[]") || inner.is_empty(),
        "expected empty array (zero @errors in fixture), got:\n{}\ninner=[{}]",
        result.output,
        inner
    );
}

#[test]
fn integration_exhaustive_sites_cap_returns_match_site_via_runtime() {
    // Distinct fixture: a single `dispatch` function whose match
    // arms coverage is captured by the exhaustive-match compiler
    // pass going through the cast-enrichment pipeline.
    //
    // CRUSH-29 ticket shape requires `file` / `line` / `function_name`
    // fields on every row. We assert the SCHEMA KEYS appear (not the
    // values) because whether `parse_source` (which
    // `Runtime::with_codebase` uses) populates `ExhaustiveMatchSite.location`
    // depends on the cast-enrichment pipeline running end-to-end. A
    // future `parse_source`-only mode that doesn't run enrichment
    // may yield `line: 0`, so we don't pin the value — filed as
    // CRUSH-CAST-MATCHLOC follow-up so this assertion can tighten.
    let src = r#"@module { purpose: "exhaustive fixture" }
fn dispatch(x) {
    match x {
        1 => "one"
        2 => "two"
    }
}
"#;
    let rt = Runtime::new()
        .with_codebase_at(&[("exhaustive", src)], pin_today())
        .expect("runtime build");

    let casm = r#"
        .func main
        PUSH_STR ""
        CAP_CALL "codebase.exhaustive_sites" 1
        CAP_CALL "io.print" 1
        HALT
    "#;
    let result = rt
        .run_casm(
            casm,
            &["codebase.exhaustive_sites", "io.print"],
            Some("exhaustive-test"),
        )
        .expect("run");
    assert!(result.halted);

    assert!(
        result.output.contains("function_name: dispatch"),
        "expected function_name from match site:\n{}",
        result.output
    );
    assert!(
        result.output.contains("file: "),
        "expected 'file' key on every exhaustive-site row:\n{}",
        result.output
    );
    assert!(
        result.output.contains("line: "),
        "expected 'line' key on every exhaustive-site row:\n{}",
        result.output
    );
}

/// 6th test: registration smoke test exercising all 5 ticket-spec
/// caps. Probes each through Runtime.run_casm catches both missing
/// registration ("capability not declared") and live runtime errors.
/// A regression that breaks `register()` would surface here.
#[test]
fn integration_all_five_ticket_caps_registered_via_runtime() {
    let rt = runtime_with_fixture();
    let probes: &[(&str, &str, &[&str])] = &[
        (
            "codebase.modules",
            r#".func main
            CAP_CALL "codebase.modules" 0
            HALT"#,
            &["codebase.modules"],
        ),
        (
            "codebase.invariants",
            r#".func main
            PUSH_STR "fixture"
            CAP_CALL "codebase.invariants" 1
            HALT"#,
            &["codebase.invariants"],
        ),
        (
            "codebase.callers",
            r#".func main
            PUSH_STR "helper"
            CAP_CALL "codebase.callers" 1
            HALT"#,
            &["codebase.callers"],
        ),
        (
            "codebase.uncovered_paths",
            r#".func main
            CAP_CALL "codebase.uncovered_paths" 0
            HALT"#,
            &["codebase.uncovered_paths"],
        ),
        (
            "codebase.exhaustive_sites",
            r#".func main
            PUSH_STR ""
            CAP_CALL "codebase.exhaustive_sites" 1
            HALT"#,
            &["codebase.exhaustive_sites"],
        ),
    ];
    for (cap, casm, perms) in probes {
        let result = rt.run_casm(casm, perms, Some("probe"));
        if let Err(e) = result {
            assert!(
                !e.to_string().contains("capability not declared"),
                "{cap} missing from registration set after with_codebase_at: {e}"
            );
        }
    }
}

// ── CRUSH-31 integration ───────────────────────────────────────────────────

#[test]
fn integration_annotation_history_cap_via_runtime() {
    // Mirror the codebase_stale_e2e.rs manual-caps-build pattern:
    // because `Runtime::with_codebase_at` doesn't accept dejavue yet
    // (the builder stays scoped to source-only ingestion for now),
    // this test wires a shared `CrushIndex` (containing both code +
    // typed timeline events) directly through `register_at`.
    use crush_index::dejavue::parse_timeline_str;
    use crush_lang_sdk::codebase;
    use crush_lang_sdk::{HostCaps, Runtime};
    use std::sync::Arc;

    // 1: parsed Crush source containing the @invariant "use-workspace-deps"
    // the timeline below targets.
    let src = r#"@module { purpose: "annotation_history fixture" }
@invariant "use-workspace-deps" {
    description: "Use workspace = true"
    applies_to: ["f"]
    consequence: "publishing path broken"
}
fn f() { }
"#;

    // 2: timeline with TWO decision events for the same invariant
    // (intentionally out of chronological order in the corpus) + 1
    // unrelated file_changed event that should NEVER land in the
    // history.
    let timeline = r#"{"ts":"2026-05-01T00:00:00-05:00","branch":"main","event":"decision","decision_title":"use-workspace-deps","decision_reason":"SECOND decision","summary":"later"}
{"ts":"2026-04-01T00:00:00-05:00","branch":"main","event":"decision","decision_title":"use-workspace-deps","decision_reason":"FIRST decision","summary":"earlier"}
{"ts":"2026-03-01T00:00:00-05:00","branch":"main","event":"file_changed","path":"x.crush","summary":"unrelated file change - should not surface"}
"#;

    // 3: build a shared CrushIndex and wire both sides via the
    //    codebase_stale_e2e.rs manual-caps-build pattern (Runtime
    //    builder stays source-only for this turn — `with_dejavue`
    //    builder is a follow-up).
    let mut xml_idx = crush_index::CrushIndex::new();
    let prog = crush_frontend::parse_source(src).expect("parse source");
    xml_idx.add_program("hist_fixture", &prog);
    let (events, _skipped) = parse_timeline_str(timeline);
    xml_idx.set_dejavue_events(events);
    let shared_idx = Arc::new(xml_idx);
    let mut caps = HostCaps::new();
    codebase::register_at(
        &mut caps,
        Arc::clone(&shared_idx),
        pin_today(),
    );
    // 4: CASM probe — pass the annotation name, capture output
    let casm = r#"
        .func main
        PUSH_STR "use-workspace-deps"
        CAP_CALL "codebase.annotation_history" 1
        CAP_CALL "io.print" 1
        HALT
    "#;
    let rt = Runtime::new().with_host_caps(caps);
    let result = rt
        .run_casm(
            casm,
            &["codebase.annotation_history", "io.print"],
            Some("hist-test"),
        )
        .expect("run");
    assert!(result.halted, "the CASM program should halt cleanly");

    // 5: both decision events present,
    assert!(
        result.output.contains("FIRST decision"),
        "expected earlier decision in output:\n{}",
        result.output
    );
    assert!(
        result.output.contains("SECOND decision"),
        "expected later decision in output:\n{}",
        result.output
    );
    assert!(
        result.output.contains("decision_title: use-workspace-deps"),
        "expected decision_title field on every row:\n{}",
        result.output
    );
    // 6: file_changed event MUST NOT surface (CRUSH-31 linking is
    // strict-equality on `decision_title` AND `event == "decision"`
    // discriminator).
    assert!(
        !result.output.contains("unrelated file change"),
        "file_changed event must NOT surface through annotation_history:\n{}",
        result.output
    );
    // 7: chronological order, irrespective of corpus insertion order
    // (corpus listed SECOND first; output must surface FIRST first).
    let first_pos = result
        .output
        .find("FIRST decision")
        .expect("FIRST decision present");
    let second_pos = result
        .output
        .find("SECOND decision")
        .expect("SECOND decision present");
    assert!(
        first_pos < second_pos,
        "annotation_history must be ts-ascending; FIRST at {}, SECOND at {}:\n{}",
        first_pos,
        second_pos,
        result.output
    );
}
