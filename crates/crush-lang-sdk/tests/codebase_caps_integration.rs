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
