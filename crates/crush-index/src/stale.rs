//! Shared `@temporary` staleness logic for `crush-frontend` (compiler-warning
//! path) and `crush-lang-sdk` (host-cap path).
//!
//! Centralising [`STALE_DAYS`] and the [`TempStaleChecker`] predicates here
//! lets the compiler warning (`W-TMP-001`) and the `codebase.*` host caps
//! stay in lockstep automatically — a future agent debugging "why does
//! `stale_temporaries()` show N but the compiler shows M" answers the
//! question by reading one file instead of comparing two near-duplicate
//! implementations drifting in parallel.
//!
//! ## Silent-skip policy
//!
//! `TemporaryNode.added` is `Option<String>`. When `added` is `None` or
//! fails to parse as `%Y-%m-%d`:
//!
//! - [`TempStaleChecker::is_stale`] returns `false`. A malformed `added`
//!   is treated as "not stale" rather than a noisy false-positive; see
//!   `crush_frontend::wip_check` history for the rationale.
//! - [`TempStaleChecker::days_old`] returns `None`. Consumers map the
//!   `None` to `Value::Null` at the host-cap layer.
//!
//! The asymmetry vs the unfiltered `codebase.temporaries()` cap, which
//! serialises whatever's in the `added` field even when it's empty, is
//! intentional: hosts that need the full row list `null`-mark the missing
//! `days_old`, while consumers of `stale_temporaries()` only ever see rows
//! that survive the silent-skip filter.

use chrono::{Duration, NaiveDate};
use crush_cast::manifest::TemporaryNode;

/// Number of days after which an un-removed `@temporary` block is
/// considered stale and the W-TMP-001 warning is emitted.
///
/// Single source of truth shared by the compiler warning
/// (`crush_frontend::wip_check`) and the host caps
/// (`crush_lang_sdk::codebase::CodebaseStaleTemporariesCap`). Callers that
/// previously imported `crush_frontend::wip_check::STALE_DAYS` continue to
/// resolve the same numeric literal via the `pub use` re-export in that
/// module.
pub const STALE_DAYS: i64 = 90;

/// `@temporary` staleness predicate with an injected "today" anchor.
///
/// Lets tests exercise the 90-day boundary deterministically by passing a
/// fixed `today` to [`new`](Self::new); production callers should pass
/// `chrono::Utc::now().date_naive()`. Earlier compiler-warning code used a
/// hardcoded date literal that rotted as wall-clock time advanced — the
/// injected-`today` design here is drift-proof.
pub struct TempStaleChecker {
    today: NaiveDate,
}

impl TempStaleChecker {
    /// Construct a checker anchored at the given `today` for the 90-day
    /// comparison. Tests pass a fixed value; production passes
    /// `chrono::Utc::now().date_naive()`.
    pub fn new(today: NaiveDate) -> Self {
        Self { today }
    }

    /// The cutoff date: `@temporary` blocks added **strictly before** this
    /// date are stale. The boundary case at exactly 90 days back is fresh
    /// (the predicate uses strict `<`, not `<=`).
    ///
    /// Kept private to this module — both consumers (`codebase.rs`,
    /// `wip_check.rs` newtype) only need [`is_stale`] / [`days_old`], and
    /// leaking `threshold()` would invite callers to re-implement the
    /// arithmetic and quietly drift out of sync.
    fn threshold(&self) -> NaiveDate {
        self.today - Duration::days(STALE_DAYS)
    }

    /// Returns `true` if `tmp` was added strictly more than `STALE_DAYS`
    /// before `self.today`.
    ///
    /// **Silent-skip policy**: if `tmp.added` is `None` or fails to parse
    /// as `%Y-%m-%d`, returns `false` (treated as fresh rather than
    /// emitting a false-positive). See the module-level docs for the
    /// rationale.
    pub fn is_stale(&self, tmp: &TemporaryNode) -> bool {
        let Some(added) = tmp.added.as_deref() else {
            return false;
        };
        let Ok(added_date) = NaiveDate::parse_from_str(added, "%Y-%m-%d") else {
            return false;
        };
        added_date < self.threshold()
    }

    /// Age of `tmp` in whole calendar days, when `added` parses as
    /// `%Y-%m-%d`. Returns `None` for missing or unparseable `added`
    /// strings — same silent-skip policy as [`is_stale`].
    ///
    /// Returned as `Option<i64>` rather than a sentinel so the host cap
    /// can serialise the "unknown" case as `Value::Null` instead of
    /// inventing a magic number; consumers reading `days_old: i64` from
    /// the `codebase.temporaries()` response need to defensively check for
    /// `Null`.
    pub fn days_old(&self, tmp: &TemporaryNode) -> Option<i64> {
        let added = tmp.added.as_deref()?;
        let added_date = NaiveDate::parse_from_str(added, "%Y-%m-%d").ok()?;
        Some((self.today - added_date).num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 20).unwrap()
    }

    fn tmp(added: Option<&str>) -> TemporaryNode {
        TemporaryNode {
            reason: "test".to_string(),
            expires_when: None,
            owner: None,
            added: added.map(str::to_string),
        }
    }

    #[test]
    fn stale_days_constant_is_90() {
        assert_eq!(STALE_DAYS, 90);
    }

    #[test]
    fn fresh_row_is_not_stale() {
        let c = TempStaleChecker::new(today());
        assert!(!c.is_stale(&tmp(Some("2026-06-19"))));
    }

    #[test]
    fn stale_row_is_stale() {
        let c = TempStaleChecker::new(today());
        assert!(c.is_stale(&tmp(Some("2026-03-21"))));
    }

    #[test]
    fn exactly_90_days_back_is_fresh() {
        // today=2026-06-20 - 90 days = 2026-03-22. Strict `<`, so 2026-03-22
        // survives the threshold check → fresh. Guards the off-by-one
        // between `<` and `<=`.
        let c = TempStaleChecker::new(today());
        assert!(!c.is_stale(&tmp(Some("2026-03-22"))));
    }

    #[test]
    fn missing_added_is_not_stale() {
        let c = TempStaleChecker::new(today());
        assert!(!c.is_stale(&tmp(None)));
    }

    #[test]
    fn unparseable_added_is_not_stale() {
        let c = TempStaleChecker::new(today());
        assert!(!c.is_stale(&tmp(Some("yesterday"))));
        assert!(!c.is_stale(&tmp(Some("2026/03/21")))); // wrong separator
        assert!(!c.is_stale(&tmp(Some("21-03-2026")))); // wrong order
    }

    #[test]
    fn days_old_counts_whole_days() {
        let c = TempStaleChecker::new(today());
        assert_eq!(c.days_old(&tmp(Some("2026-06-19"))), Some(1));
        assert_eq!(c.days_old(&tmp(Some("2026-06-15"))), Some(5));
        assert_eq!(c.days_old(&tmp(Some("2026-03-21"))), Some(91));
    }

    #[test]
    fn days_old_returns_none_when_unparseable() {
        let c = TempStaleChecker::new(today());
        assert_eq!(c.days_old(&tmp(None)), None);
        assert_eq!(c.days_old(&tmp(Some("not-a-date"))), None);
        assert_eq!(c.days_old(&tmp(Some("2026/03/21"))), None);
    }
}
