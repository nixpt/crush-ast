//! W-WIP-001 and W-TMP-001 diagnostic passes — Phase 2a of the AI-native roadmap.

use crate::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_cast::manifest::SourceLoc;
use crush_cast::Program;
use chrono::{NaiveDate, Utc};
use crush_index::stale::TempStaleChecker;

const W_WIP: &str = "W-WIP-001";
const W_TMP: &str = "W-TMP-001";

/// Number of days after which an un-removed `@temporary` block is
/// considered stale.
///
/// **Re-export** of [`crush_index::stale::STALE_DAYS`]. The literal lives
/// in `crush_index::stale` as the single source of truth shared with
/// `crush_lang_sdk::codebase` — both the compiler warning (here) and the
/// host cap resolve to the same `i64`. Public so callers can include it
/// in their own tools (e.g. the `crush` CLI help text) without growing a
/// parallel constant.
pub use crush_index::stale::STALE_DAYS;

/// Warn when a @wip node has non-empty `todo` or `unresolved` lists.
pub fn check_wip(program: &Program) -> Vec<CompilerDiagnostic> {
    let Some(wip) = &program.wip else {
        return Vec::new();
    };
    if wip.todo.is_empty() && wip.unresolved.is_empty() {
        return Vec::new();
    }
    vec![CompilerDiagnostic {
        code: W_WIP,
        severity: DiagnosticSeverity::Warning,
        message: format!(
            "module has unfinished @wip: {} todo item(s), {} unresolved",
            wip.todo.len(),
            wip.unresolved.len()
        ),
        location: SourceLoc::default(),
        hint: Some(
            "resolve all @wip.todo and @wip.unresolved before shipping".to_string(),
        ),
    }]
}

/// `@temporary` staleness checker for the W-TMP-001 compiler warning.
///
/// **Newtype wrapper** over [`crush_index::stale::TempStaleChecker`]. The
/// pure predicate struct lives in `crush_index::stale` so the compiler
/// warning (here) and the host caps (`crush_lang_sdk::codebase`) cannot
/// drift out of sync — both reference the same `is_stale` semantics. This
/// wrapper carries the diagnostic-shape conversion that `crush-index`
/// cannot implement itself (the shared module doesn't — and shouldn't —
/// depend on `crush_frontend`'s `CompilerDiagnostic` type).
///
/// Tests construct a `TempChecker` directly to drive an injected `today`;
/// production callers use the free [`check_temporaries`] function below,
/// which snaps `today` to `chrono::Utc::now().date_naive()`.
pub struct TempChecker(TempStaleChecker);

impl TempChecker {
    /// Construct a checker anchored at the given `today` for the 90-day
    /// comparison. Tests pass a fixed value; production uses
    /// [`check_temporaries`].
    pub fn new(today: NaiveDate) -> Self {
        Self(TempStaleChecker::new(today))
    }

    /// Return diagnostics for every stale `@temporary` in `program`.
    ///
    /// Delegates the filter decision to
    /// [`TempStaleChecker::is_stale`](crush_index::stale::TempStaleChecker::is_stale)
    /// (single source of truth shared with the host caps). The
    /// silent-skip policy for missing / non-`%Y-%m-%d` `added` strings
    /// lives there; this method only emits the `W-TMP-001` diagnostic.
    pub fn check(&self, program: &Program) -> Vec<CompilerDiagnostic> {
        let mut diags = Vec::new();
        for tmp in &program.temporaries {
            if !self.0.is_stale(tmp) {
                continue;
            }
            // `is_stale` returned `true`, so `added` is `Some(s)` where
            // `s` parses as `%Y-%m-%d`. We re-borrow the raw string for
            // the diagnostic message (formatting the parsed date would
            // give us back the same string, but parsing-and-formatting
            // is wasted work for a field that's already shaped right).
            let added_str = tmp.added.as_deref().unwrap_or("");
            diags.push(CompilerDiagnostic {
                code: W_TMP,
                severity: DiagnosticSeverity::Warning,
                message: format!(
                    "@temporary added {added_str} is over {STALE_DAYS} days old and may be overdue for removal"
                ),
                location: SourceLoc::default(),
                hint: tmp
                    .expires_when
                    .as_ref()
                    .map(|e| format!("remove when: {e}"))
                    .or_else(|| {
                        Some("add an `expires_when` condition or remove the block".to_string())
                    }),
            });
        }
        diags
    }
}

/// Backward-compatible entry point. Production callers (e.g.
/// [`crate::check_source`]) continue to call this; it builds a [`TempChecker`]
/// whose `today` is `chrono::Utc::now().date_naive()` (the non-deprecated
/// form of `chrono::Utc::today()`, deprecated ≈0.4.20 — preferring the
/// modern form keeps `cargo update` fallout out of the `crush` build
/// output stream).
pub fn check_temporaries(program: &Program) -> Vec<CompilerDiagnostic> {
    TempChecker::new(Utc::now().date_naive()).check(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast_enrich::enrich_cast;
    use crate::parser::Parser;
    use chrono::NaiveDate;

    /// Run only the `@wip` portion of the diagnostic suite.
    ///
    /// `run_all` is a misleading name — it deliberately excludes the
    /// `@temporary` check because that one depends on wall-clock time.
    /// Tests that need to assert W-TMP behavior must call
    /// [`check_temporaries_at`] with an injected `today`. If you copy a
    /// `@temporary`-bearing fixture into a test calling `run_all`, you
    /// will silently see zero W-TMP diagnostics; that is by design, not a
    /// regression.
    fn run_all(source: &str) -> Vec<CompilerDiagnostic> {
        let mut prog = Parser::parse(source).expect("parse");
        enrich_cast(&mut prog);
        check_wip(&prog)
    }

    /// Run the W-TMP-001 check against a fixed `today` so the test is
    /// wall-clock-independent. (The free [`super::check_temporaries`] still
    /// calls `Utc::now()` and is reserved for production callers.)
    fn check_temporaries_at(source: &str, today: NaiveDate) -> Vec<CompilerDiagnostic> {
        let mut prog = Parser::parse(source).expect("parse");
        enrich_cast(&mut prog);
        TempChecker::new(today).check(&prog)
    }

    #[test]
    fn no_wip_no_diag() {
        let diags = run_all("fn f() { }");
        assert!(diags.is_empty());
    }

    #[test]
    fn wip_all_done_no_diag() {
        let diags = run_all(
            r#"
@wip {
    intent: "done"
    done: [a, b]
}
fn f() { }
"#,
        );
        assert!(diags.is_empty(), "empty todo+unresolved → no warning");
    }

    #[test]
    fn wip_with_todo_warns() {
        let diags = run_all(
            r#"
@wip {
    intent: "in progress"
    todo: [emit, tests]
}
fn f() { }
"#,
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "W-WIP-001");
        assert!(diags[0].message.contains("2 todo"));
    }

    #[test]
    fn wip_with_unresolved_warns() {
        let diags = run_all(
            r#"
@wip {
    intent: "blocked"
    unresolved: ["design question A", "design question B", "design question C"]
}
fn f() { }
"#,
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3 unresolved"));
    }

    #[test]
    fn temporary_fresh_no_diag() {
        // today=2026-06-20 → added=2026-06-01 is 19 days back → fresh.
        let today = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        let diags = check_temporaries_at(
            r#"
@temporary {
    reason: "interim solution"
    added: "2026-06-01"
}
fn f() { }
"#,
            today,
        );
        assert!(diags.is_empty(), "recent @temporary should not warn");
    }

    #[test]
    fn temporary_stale_warns() {
        // today=2026-06-20 → added=2025-12-01 is 201 days back → stale.
        let today = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        let diags = check_temporaries_at(
            r#"
@temporary {
    reason: "ancient workaround"
    added: "2025-12-01"
}
fn f() { }
"#,
            today,
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "W-TMP-001");
        assert!(diags[0].message.contains("2025-12-01"));
    }

    #[test]
    fn temporary_stale_hint_shows_expires_when() {
        // today=2026-06-20 → added=2025-11-01 is 231 days back → stale.
        let today = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        let diags = check_temporaries_at(
            r#"
@temporary {
    reason: "old hack"
    added: "2025-11-01"
    expires_when: "sorted-index lands"
}
fn f() { }
"#,
            today,
        );
        assert_eq!(diags.len(), 1);
        let hint = diags[0].hint.as_deref().unwrap_or("");
        assert!(hint.contains("sorted-index lands"));
    }

    /// `utc_today_introspection` — drives the W-TMP-001 boundary from an
    /// injected `today` date rather than a hardcoded calendar literal.
    ///
    /// Sweeps `added` across the 90-day boundary in both directions and
    /// asserts each side behaves correctly. Includes a fourth case with
    /// `today` rolled forward to 2030 to prove the boundary is genuinely
    /// parameterised — the old constant-only test could pass for the wrong
    /// reason.
    ///
    /// All four fixtures use the multi-line annotation syntax; the parser
    /// rejects single-line comma-separated fields (@temporary { k: v, k: v }
    /// form returns "Unexpected token in expression: Comma" at col 27).
    #[test]
    fn utc_today_introspection_mocks_date() {
        let today = NaiveDate::from_ymd_opt(2026, 6, 20).unwrap();
        let checker = TempChecker::new(today);

        // 1. added 91 days back (2026-03-21) → must fire W-TMP-001
        let mut old = Parser::parse(
            r#"
@temporary {
    reason: "old"
    added: "2026-03-21"
}
fn f() { }
"#,
        )
        .expect("parse");
        enrich_cast(&mut old);
        let diags = checker.check(&old);
        assert_eq!(diags.len(), 1, "added 91 days back must warn");
        assert_eq!(diags[0].code, "W-TMP-001");

        // 2. added exactly 90 days back (2026-03-22) → on the threshold,
        //    so added_date >= threshold holds → must NOT warn
        let mut boundary = Parser::parse(
            r#"
@temporary {
    reason: "boundary"
    added: "2026-03-22"
}
fn f() { }
"#,
        )
        .expect("parse");
        enrich_cast(&mut boundary);
        let diags = checker.check(&boundary);
        assert!(diags.is_empty(), "exactly 90 days back must be fresh");

        // 3. recently added (2026-06-15) → must NOT warn
        let mut recent = Parser::parse(
            r#"
@temporary {
    reason: "recent"
    added: "2026-06-15"
}
fn f() { }
"#,
        )
        .expect("parse");
        enrich_cast(&mut recent);
        let diags = checker.check(&recent);
        assert!(diags.is_empty(), "added 5 days back must be fresh");

        // 4. rolling today forward: with today=2030-06-20, threshold is
        //    2027-03-22, so a 2025-12-15 entry (~4.5 years stale) must warn.
        let future = NaiveDate::from_ymd_opt(2030, 6, 20).unwrap();
        let f_checker = TempChecker::new(future);
        let mut s = Parser::parse(
            r#"
@temporary {
    reason: "ancient"
    added: "2025-12-15"
}
fn f() { }
"#,
        )
        .expect("parse");
        enrich_cast(&mut s);
        let diags = f_checker.check(&s);
        assert_eq!(diags.len(), 1, "with future-today, ancient temp must warn");
        assert_eq!(diags[0].code, "W-TMP-001");
    }
}
