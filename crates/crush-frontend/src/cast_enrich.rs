//! CAST enrichment pass — Step 3 of the AI-native roadmap.
//!
//! After the parser builds a `Program` from annotated Crush source, this pass
//! walks the CAST and populates derived fields that the parser cannot fill:
//!
//! - `Program.exhaustive_sites` — one `ExhaustiveMatchSite` per `match`
//!   expression found in any function body.  The `covered_arms` field lists
//!   all variant names matched in that site; `type_name` is left empty until
//!   `crush-index` runs type-level analysis (Step 4).
//!
//! The pass is a pure CAST → CAST transformation; it does not produce CASM.

use crush_cast::{Expression, Function, Pattern, Program, Statement};
use crush_cast::manifest::{ExhaustiveMatchSite, SourceLoc};
use std::collections::HashMap;

/// Enrich `program` in place by populating `Program.exhaustive_sites`.
///
/// Idempotent: calling it twice does not duplicate entries (the field is
/// replaced, not appended to).
pub fn enrich_cast(program: &mut Program) {
    let mut sites: Vec<ExhaustiveMatchSite> = Vec::new();

    for (fn_name, func) in &program.functions {
        collect_sites_in_fn(fn_name, func, &mut sites);
    }

    program.exhaustive_sites = sites;
}

// ── per-function walker ──────────────────────────────────────────────────────

fn collect_sites_in_fn(fn_name: &str, func: &Function, out: &mut Vec<ExhaustiveMatchSite>) {
    collect_sites_in_stmts(fn_name, &func.body, out);
}

fn collect_sites_in_stmts(
    fn_name: &str,
    stmts: &[Statement],
    out: &mut Vec<ExhaustiveMatchSite>,
) {
    for stmt in stmts {
        collect_sites_in_stmt(fn_name, stmt, out);
    }
}

fn collect_sites_in_stmt(fn_name: &str, stmt: &Statement, out: &mut Vec<ExhaustiveMatchSite>) {
    match stmt {
        Statement::ExprStmt { expr, .. } => {
            collect_sites_in_expr(fn_name, expr, out);
        }
        Statement::VarDecl { value, .. } | Statement::Export { value, .. } => {
            collect_sites_in_expr(fn_name, value, out);
        }
        Statement::Return { value, .. } => {
            if let Some(v) = value {
                collect_sites_in_expr(fn_name, v, out);
            }
        }
        Statement::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            collect_sites_in_expr(fn_name, condition, out);
            collect_sites_in_stmts(fn_name, then_body, out);
            if let Some(eb) = else_body {
                collect_sites_in_stmts(fn_name, eb, out);
            }
        }
        Statement::While { condition, body, .. } => {
            collect_sites_in_expr(fn_name, condition, out);
            collect_sites_in_stmts(fn_name, body, out);
        }
        Statement::For { iterable, body, .. } => {
            collect_sites_in_expr(fn_name, iterable, out);
            collect_sites_in_stmts(fn_name, body, out);
        }
        Statement::TryCatch { body, handler, .. } => {
            collect_sites_in_stmts(fn_name, body, out);
            collect_sites_in_stmts(fn_name, handler, out);
        }
        Statement::Throw { value, .. } => {
            collect_sites_in_expr(fn_name, value, out);
        }
        Statement::FunctionDef { body, .. } => {
            collect_sites_in_stmts(fn_name, body, out);
        }
        Statement::SetField { target, value, .. } => {
            collect_sites_in_expr(fn_name, target, out);
            collect_sites_in_expr(fn_name, value, out);
        }
        Statement::DomMutate {
            target,
            value,
            value2,
            ..
        } => {
            collect_sites_in_expr(fn_name, target, out);
            if let Some(v) = value {
                collect_sites_in_expr(fn_name, v, out);
            }
            if let Some(v) = value2 {
                collect_sites_in_expr(fn_name, v, out);
            }
        }
        Statement::DomEventListener { target, callback, .. } => {
            collect_sites_in_expr(fn_name, target, out);
            collect_sites_in_expr(fn_name, callback, out);
        }
        // No expressions to walk in these:
        Statement::LangBlock { .. }
        | Statement::Import { .. }
        | Statement::StructDef { .. }
        | Statement::Break { .. }
        | Statement::Continue { .. }
        | Statement::AI(_) => {}
    }
}

fn collect_sites_in_expr(fn_name: &str, expr: &Expression, out: &mut Vec<ExhaustiveMatchSite>) {
    match expr {
        Expression::Match {
            expression,
            arms,
            meta,
        } => {
            // Record this match site
            let covered_arms: Vec<String> = arms
                .iter()
                .filter_map(|arm| arm_pattern_name(&arm.pattern))
                .collect();
            let has_wildcard = arms.iter().any(|arm| matches!(arm.pattern, Pattern::Wildcard));

            let location = meta_to_source_loc(meta);

            out.push(ExhaustiveMatchSite {
                // type_name is left blank — type inference is a Step 4 concern.
                // crush-index will resolve it by comparing arm names against known
                // type variant sets.
                type_name: String::new(),
                function_name: fn_name.to_string(),
                location,
                covered_arms,
                missing_arms: Vec::new(),
                has_wildcard,
            });

            // Also recurse into the discriminant and arm bodies
            collect_sites_in_expr(fn_name, expression, out);
            for arm in arms {
                collect_sites_in_stmts(fn_name, &arm.body, out);
            }
        }

        Expression::BinaryOp { left, right, .. } => {
            collect_sites_in_expr(fn_name, left, out);
            collect_sites_in_expr(fn_name, right, out);
        }
        Expression::UnaryOp { operand, .. } => {
            collect_sites_in_expr(fn_name, operand, out);
        }
        Expression::Call { args, .. } | Expression::CapabilityCall { args, .. } | Expression::Spawn { args, .. } => {
            for a in args {
                collect_sites_in_expr(fn_name, a, out);
            }
        }
        Expression::Pipeline { segments, .. } => {
            for s in segments {
                collect_sites_in_expr(fn_name, s, out);
            }
        }
        Expression::Lambda { body, .. } => {
            collect_sites_in_stmts(fn_name, body, out);
        }
        Expression::GetField { target, .. } => {
            collect_sites_in_expr(fn_name, target, out);
        }
        Expression::Range { start, end, .. } => {
            collect_sites_in_expr(fn_name, start, out);
            collect_sites_in_expr(fn_name, end, out);
        }
        Expression::Await { expression, .. } => {
            collect_sites_in_expr(fn_name, expression, out);
        }
        Expression::ArrayLiteral { elements, .. } => {
            for e in elements {
                collect_sites_in_expr(fn_name, e, out);
            }
        }
        Expression::ObjectLiteral { properties, .. } => {
            for (_, v) in properties {
                collect_sites_in_expr(fn_name, v, out);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_sites_in_expr(fn_name, target, out);
            collect_sites_in_expr(fn_name, index, out);
        }
        Expression::DomQuery { selector, .. } => {
            collect_sites_in_expr(fn_name, selector, out);
        }

        // Leaf expressions — nothing to recurse into
        Expression::IntLiteral { .. }
        | Expression::FloatLiteral { .. }
        | Expression::StringLiteral { .. }
        | Expression::BoolLiteral { .. }
        | Expression::NullLiteral { .. }
        | Expression::Var { .. }
        | Expression::Yield { .. }
        | Expression::NewStruct { .. }
        | Expression::AI(_) => {}
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Extract a human-readable name from a match arm's pattern.
///
/// Returns `None` for wildcard arms (`_`) since they don't represent a named
/// variant and would pollute the `covered_arms` list.
fn arm_pattern_name(pattern: &Pattern) -> Option<String> {
    match pattern {
        Pattern::Identifier { name } => Some(name.clone()),
        Pattern::Struct { name, .. } => Some(name.clone()),
        Pattern::Literal { .. } => None, // integer/string literals — not a variant name
        Pattern::Wildcard => None,
    }
}

/// Build a `SourceLoc` from a CAST expression's meta HashMap.
///
/// The meta stores `"line"`, `"col"`, and `"file"` as JSON values (set by the
/// compiler's `meta_at` helpers in test fixtures, or by the parser from
/// `SourceLocation`).
fn meta_to_source_loc(meta: &HashMap<String, serde_json::Value>) -> SourceLoc {
    let line = meta
        .get("line")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let col = meta
        .get("col")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let file = meta
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    SourceLoc { file, line, col }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn parse_and_enrich(source: &str) -> Program {
        let mut program = Parser::parse(source).expect("parse should succeed");
        enrich_cast(&mut program);
        program
    }

    #[test]
    fn test_match_site_recorded() {
        let source = r#"
fn classify(val) {
    match val {
        Int(n) => { return n }
        Str(s) => { return 0 }
    }
}
"#;
        let program = parse_and_enrich(source);
        assert_eq!(program.exhaustive_sites.len(), 1);
        let site = &program.exhaustive_sites[0];
        assert_eq!(site.function_name, "classify");
        assert!(site.covered_arms.contains(&"Int".to_string()));
        assert!(site.covered_arms.contains(&"Str".to_string()));
        assert!(site.type_name.is_empty(), "type_name resolved by crush-index, not here");
    }

    #[test]
    fn test_nested_match_recorded() {
        let source = r#"
fn outer(val) {
    if val {
        match val {
            Int(n) => { return n }
        }
    }
}
"#;
        let program = parse_and_enrich(source);
        assert_eq!(program.exhaustive_sites.len(), 1);
        assert_eq!(program.exhaustive_sites[0].function_name, "outer");
    }

    #[test]
    fn test_multiple_match_sites_in_one_fn() {
        let source = r#"
fn dispatch(a, b) {
    match a {
        Spawn(f) => { return 0 }
        Done(v) => { return 1 }
    }
    match b {
        Int(n) => { return n }
    }
}
"#;
        let program = parse_and_enrich(source);
        assert_eq!(program.exhaustive_sites.len(), 2);
        // Both sites come from the same function
        assert!(program.exhaustive_sites.iter().all(|s| s.function_name == "dispatch"));
        // First site: Spawn, Done
        let a_site = &program.exhaustive_sites[0];
        assert!(a_site.covered_arms.contains(&"Spawn".to_string()));
        assert!(a_site.covered_arms.contains(&"Done".to_string()));
    }

    #[test]
    fn test_wildcard_arm_not_included() {
        let source = r#"
fn check(val) {
    match val {
        Int(n) => { return 1 }
        _ => { return 0 }
    }
}
"#;
        let program = parse_and_enrich(source);
        let site = &program.exhaustive_sites[0];
        assert!(site.covered_arms.contains(&"Int".to_string()));
        // wildcard should NOT appear in covered_arms
        assert!(!site.covered_arms.iter().any(|a| a == "_" || a.is_empty()));
    }

    #[test]
    fn test_no_match_no_sites() {
        let source = r#"
fn add(a, b) {
    return a + b
}
"#;
        let program = parse_and_enrich(source);
        assert!(program.exhaustive_sites.is_empty());
    }

    #[test]
    fn test_annotations_preserved_after_enrich() {
        let source = r#"
@module {
    purpose: "enrich test"
    exports: [run]
}
@errors [Foo::Bar]
fn run() {
    match x {
        Int(n) => { return n }
    }
}
"#;
        let program = parse_and_enrich(source);
        let manifest = program.manifest.as_ref().expect("manifest should survive enrich");
        assert_eq!(manifest.purpose, "enrich test");
        let func = program.functions.get("run").expect("run should exist");
        let ann = func.annotations.as_ref().expect("annotations should survive enrich");
        assert_eq!(ann.errors, vec!["Foo::Bar"]);
        // And the match site was also recorded
        assert_eq!(program.exhaustive_sites.len(), 1);
    }
}
