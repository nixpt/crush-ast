use crush_cast::{Program, manifest::SourceLoc};

use crate::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};

/// Check mutation ordering constraints (`@must-call-before` / `@must-call-after`).
///
/// # Diagnostics emitted
///
/// - **E-MUT-001** — a function annotated with `@must-call-before [guard]` is called
///   at a site where `guard` does NOT appear earlier in the same function body.
///   Sequential scan only; cross-function call chains are out of scope.
///
/// - **E-MUT-002** — a function annotated with `@must-call-after [cleanup]` is called
///   at a site where `cleanup` does NOT appear later in the same function body.
///
/// The check is conservative (lexical order within one function body). It does not
/// follow branches or perform dataflow analysis. When calls are conditional or the
/// body spans helpers, annotate those helpers independently.
pub fn check_mutation_ordering(program: &Program) -> Vec<CompilerDiagnostic> {
    let mut diags = Vec::new();

    for (caller_name, caller_fn) in &program.functions {
        let calls: Vec<&str> = caller_fn
            .body
            .iter()
            .filter_map(call_name_in_stmt)
            .collect();

        for (callee_name, callee_fn) in &program.functions {
            let Some(ann) = &callee_fn.annotations else {
                continue;
            };

            for required_before in &ann.must_call_before {
                // Find every index where callee is called.
                for (idx, c) in calls.iter().enumerate() {
                    if *c != callee_name.as_str() {
                        continue;
                    }
                    // required_before must appear strictly before `idx`.
                    let guard_present = calls[..idx]
                        .iter()
                        .any(|c| *c == required_before.as_str());
                    if !guard_present {
                        diags.push(CompilerDiagnostic {
                            code: "E-MUT-001".into(),
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "`{callee_name}` requires `{required_before}` to be called \
                                 before it, but no prior call was found in `{caller_name}`"
                            ),
                            location: SourceLoc::default(),
                            hint: Some(format!(
                                "Add a call to `{required_before}` before calling `{callee_name}`"
                            )),
                        });
                    }
                }
            }

            for required_after in &ann.must_call_after {
                for (idx, c) in calls.iter().enumerate() {
                    if *c != callee_name.as_str() {
                        continue;
                    }
                    // required_after must appear strictly after `idx`.
                    let cleanup_present = calls[idx + 1..]
                        .iter()
                        .any(|c| *c == required_after.as_str());
                    if !cleanup_present {
                        diags.push(CompilerDiagnostic {
                            code: "E-MUT-002".into(),
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "`{callee_name}` requires `{required_after}` to be called \
                                 after it, but no subsequent call was found in `{caller_name}`"
                            ),
                            location: SourceLoc::default(),
                            hint: Some(format!(
                                "Add a call to `{required_after}` after calling `{callee_name}`"
                            )),
                        });
                    }
                }
            }
        }
    }

    diags
}

fn call_name_in_stmt(stmt: &crush_cast::Statement) -> Option<&str> {
    match stmt {
        crush_cast::Statement::ExprStmt { expr, .. } => call_name_in_expr(expr),
        crush_cast::Statement::VarDecl { value, .. } => call_name_in_expr(value),
        crush_cast::Statement::Return { value: Some(e), .. } => call_name_in_expr(e),
        _ => None,
    }
}

fn call_name_in_expr(expr: &crush_cast::Expression) -> Option<&str> {
    match expr {
        crush_cast::Expression::Call { function, .. } => Some(function.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crush_cast::{Expression, Function, Statement};
    use crush_cast::manifest::FunctionAnnotations;
    use std::collections::HashMap;

    fn make_call(name: &str) -> Statement {
        Statement::ExprStmt {
            expr: Expression::Call {
                function: name.into(),
                args: Vec::new(),
                meta: HashMap::new(),
            },
            meta: HashMap::new(),
        }
    }

    fn make_program(
        caller_body: Vec<Statement>,
        callee_name: &str,
        ann: FunctionAnnotations,
    ) -> Program {
        let mut functions = HashMap::new();
        functions.insert(
            "caller".into(),
            Function {
                params: vec![],
                body: caller_body,
                meta: HashMap::new(),
                annotations: None,
                is_async: false,
            },
        );
        functions.insert(
            callee_name.into(),
            Function {
                params: vec![],
                body: vec![],
                meta: HashMap::new(),
                annotations: Some(ann),
                is_async: false,
            },
        );
        Program {
            cast_version: "1".into(),
            entry: "caller".into(),
            lang: None,
            functions,
            ai_meta: None,
            manifest: None,
            exhaustive_sites: vec![],
            wip: None,
            temporaries: vec![],
            decisions: vec![],
        }
    }

    #[test]
    fn must_call_before_satisfied() {
        // guard → target: no diagnostic
        let body = vec![make_call("guard"), make_call("target")];
        let ann = FunctionAnnotations {
            must_call_before: vec!["guard".into()],
            ..Default::default()
        };
        let prog = make_program(body, "target", ann);
        let diags = check_mutation_ordering(&prog);
        assert!(diags.is_empty());
    }

    #[test]
    fn must_call_before_violated() {
        // target before guard → E-MUT-001
        let body = vec![make_call("target"), make_call("guard")];
        let ann = FunctionAnnotations {
            must_call_before: vec!["guard".into()],
            ..Default::default()
        };
        let prog = make_program(body, "target", ann);
        let diags = check_mutation_ordering(&prog);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "E-MUT-001");
    }

    #[test]
    fn must_call_before_guard_missing() {
        // only target, guard never called → E-MUT-001
        let body = vec![make_call("target")];
        let ann = FunctionAnnotations {
            must_call_before: vec!["guard".into()],
            ..Default::default()
        };
        let prog = make_program(body, "target", ann);
        let diags = check_mutation_ordering(&prog);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "E-MUT-001");
    }

    #[test]
    fn must_call_after_satisfied() {
        // target → cleanup: no diagnostic
        let body = vec![make_call("target"), make_call("cleanup")];
        let ann = FunctionAnnotations {
            must_call_after: vec!["cleanup".into()],
            ..Default::default()
        };
        let prog = make_program(body, "target", ann);
        let diags = check_mutation_ordering(&prog);
        assert!(diags.is_empty());
    }

    #[test]
    fn must_call_after_violated() {
        // cleanup before target → E-MUT-002
        let body = vec![make_call("cleanup"), make_call("target")];
        let ann = FunctionAnnotations {
            must_call_after: vec!["cleanup".into()],
            ..Default::default()
        };
        let prog = make_program(body, "target", ann);
        let diags = check_mutation_ordering(&prog);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "E-MUT-002");
    }

    #[test]
    fn must_call_after_cleanup_missing() {
        // target called, cleanup never appears → E-MUT-002
        let body = vec![make_call("target")];
        let ann = FunctionAnnotations {
            must_call_after: vec!["cleanup".into()],
            ..Default::default()
        };
        let prog = make_program(body, "target", ann);
        let diags = check_mutation_ordering(&prog);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "E-MUT-002");
    }

    #[test]
    fn no_annotations_no_diags() {
        let body = vec![make_call("foo"), make_call("bar")];
        let ann = FunctionAnnotations::default();
        let prog = make_program(body, "bar", ann);
        let diags = check_mutation_ordering(&prog);
        assert!(diags.is_empty());
    }
}
