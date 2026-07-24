//! End-to-end unit tests for CRUSH-31's dejavue integration layer.
//!
//! Lock for: `crates/crush-index/src/dejavue::parse_timeline_str`,
//! `crates/crush-index/src/dejavue::build_annotation_links`, and the
//! `CrushIndex::annotation_history` query method. Each test pins a
//! fixed timeline NDJSON string (no on-disk I/O — hermetic, runnable
//! in parallel).

use chrono::FixedOffset;
use crush_index::dejavue::{build_annotation_links, parse_timeline_str, DejavueEvent};
use crush_index::CrushIndex;
use std::collections::HashMap;

// ── helpers ─────────────────────────────────────────────────────────────────

/// Pinned offset for all test timestamps (CST/CDT, the timezone
/// `.dejavue/timeline.jsonl` in this repo uses).
fn offset() -> FixedOffset {
    FixedOffset::west_opt(5 * 3600).expect("fixed offset is valid")
}

/// Build a `DejavueEvent` with the supplied timestamp + decision_title.
/// Other fields left None.
fn fixed_decision(ts: &str, title: &str) -> DejavueEvent {
    let ts = chrono::DateTime::parse_from_rfc3339(ts).expect("test fixture ts is valid");
    DejavueEvent {
        ts,
        event: "decision".to_string(),
        agent: None,
        branch: None,
        commit: None,
        summary: None,
        decision_title: Some(title.to_string()),
        decision_reason: None,
        rejected_alternatives: None,
        outcome: None,
        supersedes: None,
        durability: None,
        confidence: None,
        entities: None,
        artifacts: None,
        path: None,
        diff_stat: None,
        event_type: None,
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[test]
fn parse_timeline_str_drops_malformed_lines_silently() {
    // Two well-formed events + one malformed JSON line + one empty
    // line. `skipped` count should be N=1 (the malformed JSON only;
    // empty lines are NOT counted as skipped per the parser's design
    // — they're trivial).
    let timeline = r#"
{"ts":"2026-04-01T00:00:00-05:00","branch":"main","event":"decision","decision_title":"a","summary":"first"}

{"ts":"2026-05-01T00:00:00-05:00","branch":"main","event":"decision","decision_title":"a","summary":"second"}
{ this is not valid json }
final-event-malformed-timestamp-only
"#;
    let (events, skipped) = parse_timeline_str(timeline);
    // 2 valid events; skipped = the 2 malformed lines (JSON parse AND
    // timestamp parse failures are both counted). The final "line" is
    // ALSO not valid JSON so it counts. The empty line doesn't.
    assert_eq!(skipped, 2, "expected 2 malformed lines to be silently skipped, got {skipped}");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].decision_title.as_deref(), Some("a"));
    assert_eq!(events[1].decision_title.as_deref(), Some("a"));
}

#[test]
fn parse_timeline_str_skips_events_with_unparseable_timestamps() {
    // JSON is well-formed but `ts` is not RFC 3339 — these must be
    // silently skipped (timestamp parse failure returned None from
    // `into_typed`).
    let timeline = r#"{"ts":"not-a-timestamp","event":"decision","decision_title":"a"}
{"ts":"2026-06-01T00:00:00Z","event":"decision","decision_title":"b"}
"#;
    let (events, skipped) = parse_timeline_str(timeline);
    assert_eq!(skipped, 1, "expected 1 event with bad timestamp to be silently skipped, got {skipped}");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].decision_title.as_deref(), Some("b"));
}

#[test]
fn build_annotation_links_routes_decision_events_to_matching_title() {
    let events = vec![
        fixed_decision("2026-04-01T00:00:00-05:00", "use-workspace-deps"),
        fixed_decision("2026-05-01T00:00:00-05:00", "use-workspace-deps"),
        fixed_decision("2026-06-01T00:00:00-05:00", "different-invariant"),
    ];
    let links: HashMap<String, Vec<usize>> = build_annotation_links(&events);
    assert_eq!(links.len(), 2);
    assert_eq!(links.get("use-workspace-deps").unwrap(), &vec![0, 1]);
    assert_eq!(links.get("different-invariant").unwrap(), &vec![2]);
    // Events without `event == "decision"` (e.g. file_changed) do not
    // surface through build_annotation_links at all — verified next.
}

#[test]
fn build_annotation_links_skips_non_decision_events() {
    // Construct events without going through `fixed_decision` so we
    // can put a `file_changed` event with a `decision_title` (which
    // wouldn't normally happen but tests the strict-`event`-discriminator
    // branch explicitly).
    let events = vec![
        DejavueEvent {
            ts: chrono::DateTime::parse_from_rfc3339("2026-04-01T00:00:00-05:00").unwrap(),
            event: "file_changed".to_string(),
            agent: None,
            branch: None,
            commit: None,
            summary: None,
            decision_title: Some("should-not-link".to_string()), // present, but ignored
            decision_reason: None,
            rejected_alternatives: None,
            outcome: None,
            supersedes: None,
            durability: None,
            confidence: None,
            entities: None,
            artifacts: None,
            path: None,
            diff_stat: None,
            event_type: None,
        },
        fixed_decision("2026-05-01T00:00:00-05:00", "real-decision"),
    ];
    let links = build_annotation_links(&events);
    assert_eq!(links.len(), 1);
    assert!(links.contains_key("real-decision"));
    assert!(
        !links.contains_key("should-not-link"),
        "file_changed event with populated decision_title must NOT surface in the link map"
    );
}

#[test]
fn annotation_history_returns_chronologically_ordered_decisions() {
    // Insert events in REVERSE chronological order (storage order is
    // NOT chronological) and verify `annotation_history` returns them
    // in ASCENDING ts order — verifying the explicit re-sort.
    let mut idx = CrushIndex::new();
    let events = vec![
        // index 0: 2026-06 (latest)
        fixed_decision("2026-06-01T00:00:00-05:00", "x"),
        // index 1: 2026-05
        fixed_decision("2026-05-01T00:00:00-05:00", "x"),
        // index 2: 2026-04 (earliest)
        fixed_decision("2026-04-01T00:00:00-05:00", "x"),
    ];
    idx.set_dejavue_events(events);

    let history = idx.annotation_history("x");
    assert_eq!(history.len(), 3);
    assert!(
        history[0].ts < history[1].ts,
        "history must be ts-ascending (was {:?}, {:?}, {:?})",
        history[0].ts,
        history[1].ts,
        history[2].ts
    );
    assert!(
        history[1].ts < history[2].ts,
        "history must be ts-ascending"
    );
}

#[test]
fn annotation_history_returns_empty_vec_for_unknown_name() {
    let mut idx = CrushIndex::new();
    idx.set_dejavue_events(vec![fixed_decision(
        "2026-04-01T00:00:00-05:00",
        "real-decision",
    )]);
    let history = idx.annotation_history("completely-unrelated");
    assert!(
        history.is_empty(),
        "expected empty Vec for an annotation name not in the link map"
    );
}

#[test]
fn annotation_history_filters_non_decision_events_via_link_layer() {
    // Verify that even when non-decision events are in the corpus with
    // matching decision_title (artificial case), they don't surface
    // through annotation_history because build_annotation_links uses
    // the `event == "decision"` discriminator.
    let mut idx = CrushIndex::new();
    let events = vec![
        // file_changed with a decision_title that should be IGNORED
        DejavueEvent {
            ts: chrono::DateTime::parse_from_rfc3339("2026-04-01T00:00:00-05:00").unwrap(),
            event: "file_changed".to_string(),
            decision_title: Some("ignored-title".to_string()),
            agent: None,
            branch: None,
            commit: None,
            summary: None,
            decision_reason: None,
            rejected_alternatives: None,
            outcome: None,
            supersedes: None,
            durability: None,
            confidence: None,
            entities: None,
            artifacts: None,
            path: None,
            diff_stat: None,
            event_type: None,
        },
        fixed_decision("2026-05-01T00:00:00-05:00", "real-title"),
    ];
    idx.set_dejavue_events(events);

    let ignored = idx.annotation_history("ignored-title");
    assert!(
        ignored.is_empty(),
        "annotation_history must NOT surface non-decision events, even when they have a decision_title"
    );
    let real = idx.annotation_history("real-title");
    assert_eq!(real.len(), 1);
    assert_eq!(real[0].decision_title.as_deref(), Some("real-title"));
}
