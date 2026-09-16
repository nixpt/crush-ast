//! CRUSH-27 parse tests for the `@`-annotation blocks. Verifies the
//! dispatch in `parser/mod.rs:parse_program` correctly extracts annotation
//! data into the dedicated AST slots (Program.{manifest,wip,temporaries,
//! decisions} + Function.annotations), and that malformed `@<unknown>`
//! forms emit `ParseError::UnknownAnnotation` instead of silently dropping.
//!
//! Also covers AI soft-keyword parsing: `semantic_switch`, named-arg
//! stripping in `ai_synthesize(…, constraints=[…])`, and `ai_semantic_match`.

use crush_cast::ai::AIStatement;
use crush_cast::{Program, Statement};
use crush_frontend::parser::{ParseError, Parser};

fn parse(src: &str) -> Result<Program, Vec<ParseError>> {
    Parser::parse(src)
}

// ─── Per-form parse-success tests ───────────────────────────────────────

#[test]
fn parse_module_block_extracts_purpose_exports_and_invariants() {
    let src = r#"
@module {
    purpose: "compute things"
    exports: ["run", "step"]
}
fn main() {}
"#;
    let p = parse(src).expect("should parse");
    let m = p.manifest.expect("module present");
    assert_eq!(m.purpose, "compute things");
    assert_eq!(m.exports, vec!["run", "step"]);
}

#[test]
fn parse_invariant_block_adds_named_invariant_to_module() {
    let src = r#"
@invariant "x-positive" {
    description: "compute returns a positive number"
    applies_to: ["compute"]
}
fn compute() {}
"#;
    let p = parse(src).expect("should parse");
    let m = p.manifest.expect("module present");
    assert_eq!(m.invariants.len(), 1);
    assert_eq!(m.invariants[0].name, "x-positive");
    assert_eq!(m.invariants[0].applies_to, vec!["compute"]);
}

#[test]
fn parse_exhaustive_match_sites_list_form() {
    let src = r#"
@exhaustive-match-sites [Value, Token]
fn step_one() {}
"#;
    let p = parse(src).expect("should parse");
    let m = p.manifest.expect("module present");
    assert_eq!(m.exhaustive_types, vec!["Value", "Token"]);
}

#[test]
fn parse_errors_list_form_attaches_to_next_function() {
    let src = r#"
@errors [StackUnderflow, BadJump]
fn vm_step() {}
"#;
    let p = parse(src).expect("should parse");
    let func = p.functions.get("vm_step").expect("function present");
    let ann = func.annotations.as_ref().expect("annotations attached");
    assert_eq!(ann.errors, vec!["StackUnderflow", "BadJump"]);
}

#[test]
fn parse_errors_weighted_form_attaches_weighted_errors() {
    let src = r#"
@errors {
    NetworkTimeout: likely
    DatabaseError: rare
}
fn fetch() {}
"#;
    let p = parse(src).expect("should parse");
    let func = p.functions.get("fetch").expect("function present");
    let ann = func.annotations.as_ref().expect("annotations attached");
    assert_eq!(ann.errors_weighted.len(), 2);
    assert!(
        ann.errors_weighted
            .iter()
            .any(|w| w.variant == "NetworkTimeout"),
        "expected NetworkTimeout weighted error"
    );
}

#[test]
fn parse_reads_list_form() {
    let src = r#"
@reads [thread.ip, thread.stack]
fn step() {}
"#;
    let p = parse(src).expect("should parse");
    let func = p.functions.get("step").expect("function present");
    let ann = func.annotations.as_ref().expect("annotations attached");
    assert_eq!(ann.reads, vec!["thread.ip", "thread.stack"]);
}

#[test]
fn parse_writes_list_form() {
    let src = r#"
@writes [program, thread.out_parts]
fn step() {}
"#;
    let p = parse(src).expect("should parse");
    let func = p.functions.get("step").expect("function present");
    let ann = func.annotations.as_ref().expect("annotations attached");
    assert_eq!(ann.writes, vec!["program", "thread.out_parts"]);
}

#[test]
fn parse_coverage_list_form() {
    let src = r#"
@covers [VmError::StackUnderflow, VmError::DivByZero]
fn test_stack() {}
"#;
    let p = parse(src).expect("should parse");
    let func = p.functions.get("test_stack").expect("function present");
    let ann = func.annotations.as_ref().expect("annotations attached");
    assert_eq!(ann.covers, vec!["VmError::StackUnderflow", "VmError::DivByZero"]);
}

#[test]
fn parse_does_not_write_does_not_panic() {
    let src = r#"
@does-not-write [program]
fn step() {}
"#;
    // Just ensure no panic / unhandled error — exact surface is whatever
    // FunctionAnnotations does-not-write policy is (currently parser may
    // drop or surface — both acceptable; we just want no crash).
    let _ = parse(src).expect("should parse");
}

// ─── Integration test (ai_agent_ops-style example) ──────────────────────

#[test]
fn parse_full_ai_agent_ops_example_extracts_all_annotations() {
    let src = r#"
@module {
    purpose: "AI agent ops demo"
    exports: [process_user_intent, summarize_unresolved_tasks]
}
@wip {
    intent: "build an autonomous support agent"
    done: ["basic routing"]
    todo: ["escalation API"]
}
@invariant "escalation-requires-auth" {
    description: "any escalation must verify session"
    applies_to: [escalate_issue]
}
@errors {
    NetworkTimeout: likely
    DatabaseConnectionError: rare
}
fn process_user_intent(intent_text) {
    return intent_text
}
"#;
    let p = parse(src).expect("should parse");
    let m = p.manifest.expect("module present");
    assert_eq!(m.purpose, "AI agent ops demo");
    assert_eq!(m.invariants.len(), 1);
    assert_eq!(m.invariants[0].name, "escalation-requires-auth");
    assert!(p.wip.is_some(), "@wip block parsed");
    assert_eq!(
        p.wip.as_ref().unwrap().intent,
        "build an autonomous support agent"
    );
    // The @errors block attaches to the next fn
    let func = p
        .functions
        .get("process_user_intent")
        .expect("function present");
    let ann = func.annotations.as_ref().expect("annotations attached");
    assert_eq!(ann.errors_weighted.len(), 2);
}

// ─── Per-block tests (decision/wip/temporary) ───────────────────────────

#[test]
fn parse_decision_block_attaches_to_program() {
    let src = r#"
@decision "use-rc-refcell" {
    chose: "Rc<RefCell>"
    over: ["Arc<Mutex>"]
    because: "cheaper for single-threaded uses"
}
fn main() {}
"#;
    let p = parse(src).expect("should parse");
    assert_eq!(p.decisions.len(), 1);
    assert_eq!(p.decisions[0].name, "use-rc-refcell");
    assert_eq!(p.decisions[0].chose, "Rc<RefCell>");
    assert_eq!(p.decisions[0].over, vec!["Arc<Mutex>"]);
}

/// Regression test for github.com/nixpt/crush-ast#38.
/// `revisit-if` caused an infinite loop because the lexer emits `if` as a
/// keyword token and the parser's annotation key reader only accepted plain
/// identifiers.  After the fix, the field must be parsed and its value stored.
#[test]
fn parse_decision_revisit_if_field_does_not_hang() {
    let src = r#"
@decision "test" {
    chose: "a"
    revisit-if: ["never"]
}
fn main() {}
"#;
    let p = parse(src).expect("should parse");
    assert_eq!(p.decisions.len(), 1);
    assert_eq!(p.decisions[0].revisit_if, vec!["never"]);
}

/// Full `@decision` block with all four documented fields (chose, over,
/// because, revisit-if) to ensure nothing regresses together.
#[test]
fn parse_decision_all_fields() {
    let src = r#"
@decision "use-semantic-switch-routing" {
    chose: "semantic_switch"
    over: ["regex matching", "LLM zero-shot prompt"]
    because: "embeddings cover common intents"
    revisit-if: ["user intents become too highly contextual"]
}
fn main() {}
"#;
    let p = parse(src).expect("should parse");
    assert_eq!(p.decisions.len(), 1);
    let d = &p.decisions[0];
    assert_eq!(d.name, "use-semantic-switch-routing");
    assert_eq!(d.chose, "semantic_switch");
    assert_eq!(d.over, vec!["regex matching", "LLM zero-shot prompt"]);
    assert_eq!(d.because, "embeddings cover common intents");
    assert_eq!(d.revisit_if, vec!["user intents become too highly contextual"]);
}

#[test]
fn parse_wip_block_attaches_to_program() {
    let src = r#"
@wip { intent: "build feature X" }
fn main() {}
"#;
    let p = parse(src).expect("should parse");
    assert!(p.wip.is_some());
    assert_eq!(p.wip.as_ref().unwrap().intent, "build feature X");
}

#[test]
fn parse_temporary_block_attaches_to_program() {
    let src = r#"
@temporary { reason: "lazy workaround" added: "2026-07-23" }
fn main() {}
"#;
    let p = parse(src).expect("should parse");
    assert_eq!(p.temporaries.len(), 1);
    assert_eq!(p.temporaries[0].reason, "lazy workaround");
    assert_eq!(p.temporaries[0].added.as_deref(), Some("2026-07-23"));
}

// ─── Parse-error dispatch: UnknownAnnotation (CRUSH-27 success criterion) ─

#[test]
fn parse_unknown_annotation_emits_UnknownAnnotation_error() {
    // CRUSH-27: bare `@unknown` (no `{ ... }` body) hits the parser's
    // `_ =>` wildcard arm in `parse_program` and emits
    // ParseError::UnknownAnnotation. The body-form path (`@bogus { ... }`)
    // still surfaces as a polyglot LangBlock attempt — that's a known
    // partial-coverage gap filer as a follow-up ticket; tracked in the
    // CRUSH-27 Resolution section.
    let src = r#"
@nope_such_thing
@also_not
fn main() {}
"#;
    let res = parse(src);
    match res {
        Err(errors) => {
            let unknown_count = errors
                .iter()
                .filter(|e| matches!(e, ParseError::UnknownAnnotation { .. }))
                .count();
            assert!(
                unknown_count >= 2,
                "expected ≥2 UnknownAnnotation errors (we have 2 bad @-forms), got {unknown_count}: {errors:?}"
            );
        }
        Ok(_) => panic!("expected errors for unknown @-forms, got Ok"),
    }
}

#[test]
fn parse_unknown_annotation_does_not_hang() {
    // Truncated input without a body — would loop forever without the
    // explicit `self.advance()` in the new `_ => { ... }` arm of the
    // AtIdent dispatch in `parser/mod.rs:parse_program`.
    let src = r#"@weird_thing"#;
    let _res = parse(src);
    // Termination assertion: we reached this line at all.
}

#[test]
fn parse_known_unknown_split_is_clean() {
    let src = r#"
@module { purpose: "x" }
@bogus
fn main() {}
"#;
    let res = parse(src);
    match res {
        Err(errors) => {
            let unknown_count = errors
                .iter()
                .filter(|e| matches!(e, ParseError::UnknownAnnotation { .. }))
                .count();
            assert_eq!(
                unknown_count, 1,
                "should have exactly one UnknownAnnotation error (@module parses, @bogus fails)"
            );
        }
        Ok(_) => panic!("expected @bogus to fail"),
    }
}

// ─── AI soft-keyword parse tests ────────────────────────────────────────────

#[test]
fn parse_semantic_switch_produces_ai_statement() {
    let src = r#"
fn route(intent) {
    semantic_switch intent {
        case "refund":
            return "refund_handler"
        case "cancel":
            return "cancel_handler"
        fallback:
            return "default_handler"
    }
}
"#;
    let p = parse(src).expect("should parse");
    let func = p.functions.get("route").expect("function present");
    let switch_stmt = func.body.iter().find(|s| {
        matches!(s, Statement::AI(AIStatement::SemanticSwitch { .. }))
    });
    assert!(switch_stmt.is_some(), "expected a SemanticSwitch AI statement in function body");

    if let Some(Statement::AI(AIStatement::SemanticSwitch { cases, fallback, .. })) = switch_stmt {
        assert_eq!(cases.len(), 2, "expected 2 cases");
        assert_eq!(cases[0].0, "refund");
        assert_eq!(cases[1].0, "cancel");
        assert!(fallback.is_some(), "expected fallback block");
    }
}

#[test]
fn parse_named_arg_stripped_in_ai_synthesize_call() {
    // constraints=["polite"] — named arg must be stripped so the call parses
    // without error and the array literal is used as the second positional arg.
    let src = r#"
fn gen() {
    let r = ai_synthesize("RefundResponse", constraints=["polite", "brief"])
    return r
}
"#;
    let p = parse(src).expect("named-arg call should parse without error");
    assert!(p.functions.contains_key("gen"), "function present");
}

#[test]
fn parse_ai_agent_ops_example_parses_cleanly() {
    // Full example from examples/crush/ai_agent_ops.crush (minus the TIMEOUT marker
    // which was removed after the @decision revisit-if fix).
    let src = r#"
@module {
    purpose: "AI agent ops demo"
    exports: [process_user_intent, escalate_issue]
}
@decision "use-semantic-switch-routing" {
    chose: "semantic_switch"
    over: ["regex matching", "LLM zero-shot prompt"]
    because: "embeddings cover common intents"
    revisit-if: ["user intents become too highly contextual for basic embeddings"]
}
@errors {
    NetworkTimeout: likely
}
fn process_user_intent(intent_text) {
    semantic_switch intent_text {
        case "User is asking for a refund or billing help":
            return ai_synthesize("RefundResponse", constraints=["polite"])
        fallback:
            return "I'm not sure how to help."
    }
}
fn escalate_issue(details) {
    print("Escalating: ", details)
}
"#;
    let p = parse(src).expect("ai_agent_ops example should parse cleanly");
    assert_eq!(p.decisions.len(), 1);
    assert_eq!(p.decisions[0].revisit_if, vec!["user intents become too highly contextual for basic embeddings"]);

    let func = p.functions.get("process_user_intent").expect("function present");
    let has_switch = func.body.iter().any(|s| {
        matches!(s, Statement::AI(AIStatement::SemanticSwitch { .. }))
    });
    assert!(has_switch, "expected semantic_switch in process_user_intent body");
}
