//! Exhaustiveness analysis pass — Step 7 of the AI-native roadmap.
//!
//! Runs after the CAST enrichment pass (`cast_enrich`) and emits diagnostics
//! for match sites that use wildcard arms inside modules that declared
//! `@exhaustive_types`.
//!
//! ## Rationale
//!
//! A wildcard arm (`_ => { ... }`) in a match expression silently handles any
//! variant the programmer forgot to list.  When a module explicitly declares
//! `exhaustive_types: [Foo, Bar]` in its `@module` annotation, every match
//! over those types should name each variant explicitly — a wildcard may hide
//! a gap that will become a runtime bug when a new variant is added.
//!
//! ## Diagnostic emitted
//!
//! Code `E-EXH-001`, severity `Warning`:
//! > match in 'fn_name' uses a wildcard arm — exhaustive coverage of
//! > [Foo, Bar] (declared in @exhaustive_types) may be incomplete
//!
//! ## Limitations (known, to be resolved in later steps)
//!
//! - Without type inference, `type_name` in each site is empty.  We therefore
//!   warn on ANY wildcard match in the module, not just matches on tracked types.
//!   Once type inference lands, the check can be narrowed to specific types.
//! - `missing_arms` is not yet populated (that requires knowing all variants of
//!   the type, which needs the type registry — also a later step).

use crate::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_cast::Program;

/// Diagnostic code for wildcard-in-exhaustive-match.
const WILDCARD_IN_EXHAUSTIVE: &str = "E-EXH-001";

/// Check `program` for exhaustiveness issues and return any diagnostics found.
///
/// Returns an empty `Vec` if the program has no `@module` annotation or no
/// `exhaustive_types` declared — the check is opt-in at the module level.
pub fn check_exhaustiveness(program: &Program) -> Vec<CompilerDiagnostic> {
    let mut diags: Vec<CompilerDiagnostic> = Vec::new();

    let Some(manifest) = &program.manifest else {
        return diags;
    };
    if manifest.exhaustive_types.is_empty() {
        return diags;
    }

    let types_display = manifest.exhaustive_types.join(", ");

    for site in &program.exhaustive_sites {
        if !site.has_wildcard {
            continue;
        }
        diags.push(CompilerDiagnostic {
            code: WILDCARD_IN_EXHAUSTIVE,
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "match in '{}' uses a wildcard arm — exhaustive coverage of [{}] \
                 (declared in @exhaustive_types) may be incomplete",
                site.function_name, types_display
            ),
            location: site.location.clone(),
            hint: Some(
                "replace `_ => {{ ... }}` with an explicit arm for each variant \
                 so new variants cause a compile error here"
                    .to_string(),
            ),
        });
    }

    diags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::cast_enrich::enrich_cast;

    fn run(source: &str) -> Vec<CompilerDiagnostic> {
        let mut prog = Parser::parse(source).expect("parse should succeed");
        enrich_cast(&mut prog);
        check_exhaustiveness(&prog)
    }

    #[test]
    fn no_manifest_no_diags() {
        let diags = run(r#"
fn f(x) {
    match x {
        Int(n) => { return n }
        _ => { return 0 }
    }
}
"#);
        assert!(diags.is_empty(), "no @module → no diagnostics");
    }

    #[test]
    fn manifest_without_exhaustive_types_no_diags() {
        let diags = run(r#"
@module { purpose: "simple" }
fn f(x) {
    match x {
        _ => { return 0 }
    }
}
"#);
        assert!(diags.is_empty(), "empty exhaustive_types → no diagnostics");
    }

    #[test]
    fn wildcard_in_tracked_module_warns() {
        let diags = run(r#"
@module {
    purpose: "needs exhaustive matches"
    exhaustive_types: [Color]
}
fn paint(c) {
    match c {
        Red(v) => { return 1 }
        _ => { return 0 }
    }
}
"#);
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.code, "E-EXH-001");
        assert!(matches!(d.severity, DiagnosticSeverity::Warning));
        assert!(d.message.contains("paint"), "message should mention fn name");
        assert!(d.message.contains("Color"), "message should mention tracked type");
        assert!(d.hint.is_some());
    }

    #[test]
    fn explicit_only_match_no_diags() {
        let diags = run(r#"
@module {
    purpose: "fully explicit"
    exhaustive_types: [Shape]
}
fn classify(s) {
    match s {
        Circle(r) => { return 1 }
        Rect(w)   => { return 2 }
    }
}
"#);
        assert!(diags.is_empty(), "no wildcard → no diagnostics");
    }

    #[test]
    fn multiple_wildcard_sites_get_multiple_diags() {
        let diags = run(r#"
@module {
    purpose: "two wildcards"
    exhaustive_types: [Event]
}
fn handle_a(e) {
    match e {
        Click(x) => { return 1 }
        _ => { return 0 }
    }
}
fn handle_b(e) {
    match e {
        Hover(x) => { return 2 }
        _ => { return 0 }
    }
}
"#);
        assert_eq!(diags.len(), 2);
        let fns: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(fns.iter().any(|m| m.contains("handle_a")));
        assert!(fns.iter().any(|m| m.contains("handle_b")));
    }

    #[test]
    fn diagnostic_display_includes_code_and_severity() {
        let diags = run(r#"
@module {
    purpose: "display test"
    exhaustive_types: [X]
}
fn f(v) {
    match v {
        A(x) => { return 1 }
        _ => { return 0 }
    }
}
"#);
        assert_eq!(diags.len(), 1);
        let s = diags[0].to_string();
        assert!(s.contains("E-EXH-001"));
        assert!(s.contains("warning"));
        assert!(s.contains("hint:"));
    }
}
