//! W-WIP-001 and W-TMP-001 diagnostic passes — Phase 2a of the AI-native roadmap.

use crate::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_cast::manifest::SourceLoc;
use crush_cast::Program;

const W_WIP: &str = "W-WIP-001";
const W_TMP: &str = "W-TMP-001";

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

/// Warn when a @temporary node's `added` date is older than 90 days
/// (ISO 8601 comparison; threshold = 2026-03-18 relative to today 2026-06-17).
pub fn check_temporaries(program: &Program) -> Vec<CompilerDiagnostic> {
    const STALE_BEFORE: &str = "2026-03-18";
    let mut diags = Vec::new();
    for tmp in &program.temporaries {
        let Some(added) = &tmp.added else {
            continue;
        };
        if added.as_str() >= STALE_BEFORE {
            continue;
        }
        diags.push(CompilerDiagnostic {
            code: W_TMP,
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "@temporary added {added} is over 90 days old and may be overdue for removal"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast_enrich::enrich_cast;
    use crate::parser::Parser;

    fn run_all(source: &str) -> Vec<CompilerDiagnostic> {
        let mut prog = Parser::parse(source).expect("parse");
        enrich_cast(&mut prog);
        let mut diags = check_wip(&prog);
        diags.extend(check_temporaries(&prog));
        diags
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
        let diags = run_all(
            r#"
@temporary {
    reason: "interim solution"
    added: "2026-06-01"
}
fn f() { }
"#,
        );
        assert!(diags.is_empty(), "recent @temporary should not warn");
    }

    #[test]
    fn temporary_stale_warns() {
        let diags = run_all(
            r#"
@temporary {
    reason: "ancient workaround"
    added: "2025-12-01"
}
fn f() { }
"#,
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "W-TMP-001");
        assert!(diags[0].message.contains("2025-12-01"));
    }

    #[test]
    fn temporary_stale_hint_shows_expires_when() {
        let diags = run_all(
            r#"
@temporary {
    reason: "old hack"
    added: "2025-11-01"
    expires_when: "sorted-index lands"
}
fn f() { }
"#,
        );
        assert_eq!(diags.len(), 1);
        let hint = diags[0].hint.as_deref().unwrap_or("");
        assert!(hint.contains("sorted-index lands"));
    }
}
