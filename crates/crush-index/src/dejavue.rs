//! Typed dejavue timeline events — CRUSH-31 introduces parsed
//! [`DejavueEvent`] records (replacing the raw `Vec<String>` NDJSON-line
//! buffer that [`CrushIndex::load_dejavue`] used to populate).
//!
//! Loading is permissive: malformed JSON lines or unparseable timestamps
//! silently increment a skipped counter rather than aborting. The
//! timeline is a long-running ground-truth stream that's hand-edited
//! across agents + humans; strict required-field schemas would
//! fragment loading every time a new event-type is introduced.
//!
//! The annotation-event join (which `Invitation::Invariant.name` lines
//! up against `event.decision_title`) uses strict byte-equality —
//! follows the established Module-singleton and Annotation-dedup
//! pattern in `crush-index` (CRUSH-28).
//!
//! Schema sketch (from `.dejavue/timeline.jsonl` in this repo, sampled):
//!
//! ```json
//! {"ts":"2026-06-15T23:58:05-05:00","branch":"main","commit":"2311fa9",
//!  "agent":"unknown","event":"init","summary":"Initialized..."}
//! {"ts":"2026-06-15T23:59:05-05:00","branch":"main","commit":"2311fa9",
//!  "agent":"unknown","event":"decision","event_type":"decision",
//!  "decision_title":"Use workspace = true for all internal crate deps",
//!  "decision_reason":"...","summary":"..."}
//! {"ts":"2026-06-16T00:10:58-05:00","agent":"git-hook","event":"file_changed",
//!  "path":".dejavue/context.md","branch":"main","commit":"dda172d",
//!  "diff_stat":"...","summary":"..."}
//! ```

use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use std::collections::HashMap;

/// A single parsed timeline event.
///
/// Every optional field uses `#[serde(default)]` so a new event-type
/// landing in the corpus doesn't fragment loading. Consumers should
/// branch on the `event` discriminator and pull the type-specific
/// fields they care about (`decision_title` for decisions, `path`
/// for file_changed, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct DejavueEvent {
    /// Event timestamp (RFC 3339 with offset).
    pub ts: DateTime<FixedOffset>,
    /// Event discriminator — `"init"` / `"decision"` / `"file_changed"`
    /// / future variants. Permissive: any string is accepted
    /// (validation deferred to consumers).
    pub event: String,
    /// Agent that recorded the event. `None` for human-only edits;
    /// `"unknown"` for AI-driven entries before the agent name is
    /// canonicalised.
    pub agent: Option<String>,
    /// Git branch at the time of the event.
    pub branch: Option<String>,
    /// Git commit hash; `None` for synthetic events.
    pub commit: Option<String>,
    /// Free-form summary line. Many consumers prefer this over
    /// `decision_title` for human-readable output.
    pub summary: Option<String>,
    /// Decision-specific title — the natural join key against
    /// `Annotation::Invariant.name` (see
    /// [`CrushIndex::annotation_history`]).
    pub decision_title: Option<String>,
    /// Decision-specific reason prose.
    pub decision_reason: Option<String>,
    /// Decision-specific rejected alternatives.
    pub rejected_alternatives: Option<Vec<String>>,
    /// Decision-specific outcome field. Often empty.
    pub outcome: Option<String>,
    /// Decision-specific supersedes marker.
    pub supersedes: Option<String>,
    /// Decision-specific durability tag.
    pub durability: Option<String>,
    /// Decision-specific confidence tag.
    pub confidence: Option<String>,
    /// Decision-specific entities referenced.
    pub entities: Option<Vec<String>>,
    /// Decision-specific artifacts produced.
    pub artifacts: Option<Vec<String>>,
    /// `file_changed`-specific path.
    pub path: Option<String>,
    /// `file_changed`-specific diff stat string.
    pub diff_stat: Option<String>,
    /// Legacy classification string from old-format events (kept
    /// for back-compat with timeline entries that pre-date the
    /// `event` discriminator).
    pub event_type: Option<String>,
}

/// Parse `.dejavue/timeline.jsonl` from the project's working directory.
///
/// Returns `(typed_events, skipped_count)`. The skipped-count is the
/// number of non-empty lines that were either (a) malformed JSON, or
/// (b) JSON that didn't carry a parseable RFC 3339 timestamp. Empty
/// lines are not counted as skipped (they're trivial).
pub fn parse_timeline_file() -> (Vec<DejavueEvent>, usize) {
    let content = match std::fs::read_to_string(".dejavue/timeline.jsonl") {
        Ok(c) => c,
        Err(_) => return (Vec::new(), 0),
    };
    parse_timeline_str(&content)
}

/// Same as [`parse_timeline_file`] but takes content directly. Exposed
/// for unit tests so we don't have to manipulate the on-disk
/// `.dejavue/` state during isolated test runs (chdir across the
/// process is fragile).
pub fn parse_timeline_str(content: &str) -> (Vec<DejavueEvent>, usize) {
    let mut events: Vec<DejavueEvent> = Vec::new();
    let mut skipped = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<RawEvent>(trimmed) {
            Ok(raw) => match raw.into_typed() {
                Some(ev) => events.push(ev),
                None => skipped += 1,
            },
            Err(_) => skipped += 1,
        }
    }
    (events, skipped)
}

/// Build `annotation_name -> [event_idx, ...]` for an `events: &[DejavueEvent]`
/// corpus. Strategy: strict byte-equality on `decision_title` against
/// the annotation name. Non-decision events (`file_changed`, `init`,
/// etc.) are NOT routed through this link; the agent querying
/// `codebase.annotation_history` is interested in the decision history,
/// not all events at the timestamp.
///
/// Indexes are positions into the input slice; we insert
/// chronologically into `CrushIndex::dejavue_events` so positions are
/// stable across the index's lifetime. Future SQLite migration will
/// store `(text, event_id)` FK pairs instead of positions, but the
/// public API doesn't change.
pub fn build_annotation_links(
    events: &[DejavueEvent],
) -> HashMap<String, Vec<usize>> {
    let mut links: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, ev) in events.iter().enumerate() {
        if ev.event != "decision" {
            continue;
        }
        if let Some(title) = &ev.decision_title {
            links.entry(title.clone()).or_default().push(idx);
        }
    }
    links
}

// ──── serde-private raw-event intermediate ──────────────────────────────────

/// `serde_json::from_str` doesn't parse the timestamp shape into a
/// `DateTime<FixedOffset>` directly when given a plain string (it
/// would need a custom deserializer). The two-step approach (raw
/// struct + manual timestamp parse) keeps the field-flat shape
/// readable AND lets `into_typed` skip events with malformed
/// timestamps silently.
#[derive(Debug, Deserialize)]
struct RawEvent {
    ts: String,
    #[serde(default)]
    event: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    decision_title: Option<String>,
    #[serde(default)]
    decision_reason: Option<String>,
    #[serde(default)]
    rejected_alternatives: Option<Vec<String>>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    supersedes: Option<String>,
    #[serde(default)]
    durability: Option<String>,
    #[serde(default)]
    confidence: Option<String>,
    #[serde(default)]
    entities: Option<Vec<String>>,
    #[serde(default)]
    artifacts: Option<Vec<String>>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    diff_stat: Option<String>,
    #[serde(default)]
    event_type: Option<String>,
}

impl RawEvent {
    fn into_typed(self) -> Option<DejavueEvent> {
        let ts = DateTime::parse_from_rfc3339(&self.ts).ok()?;
        Some(DejavueEvent {
            ts,
            event: self.event,
            agent: self.agent,
            branch: self.branch,
            commit: self.commit,
            summary: self.summary,
            decision_title: self.decision_title,
            decision_reason: self.decision_reason,
            rejected_alternatives: self.rejected_alternatives,
            outcome: self.outcome,
            supersedes: self.supersedes,
            durability: self.durability,
            confidence: self.confidence,
            entities: self.entities,
            artifacts: self.artifacts,
            path: self.path,
            diff_stat: self.diff_stat,
            event_type: self.event_type,
        })
    }
}
