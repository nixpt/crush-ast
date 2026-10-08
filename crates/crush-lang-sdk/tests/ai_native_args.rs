//! CRUSH-156: `ai_native.*` caps receive their compiled arguments — the
//! payload plus the stack operands — identically on CVM1 (scheduler),
//! PortableVm and FastVM (`resolve_host_request`).

use crush_cast::ai::{AIExpression, AIStatement, Priority};
use crush_cast::{CastType, Expression, Statement};
use crush_lang_sdk::{HostCap, HostCapSpec, HostCapsBuilder};
use crush_vm::fastvm::{FastYield, HostRequest, resolve_host_request};
use crush_vm::portable_vm::PortableVm;
use crush_vm::value::RuntimeValue;
use crush_vm::vm::{Quotas, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn s(v: &str) -> Expression {
    Expression::StringLiteral {
        value: v.into(),
        meta: HashMap::new(),
    }
}

fn int(v: i64) -> Expression {
    Expression::IntLiteral {
        value: v,
        meta: HashMap::new(),
    }
}

/// `fn main() { <goal>; let q = …; let m = …; let y = …; test.capture([q, m, y]); }`
/// with the three placeholders replaced by AI expressions — the Crush text
/// parser has no AI expression syntax; CAST producers (walkers, CSON) do.
fn program() -> crush_cast::Program {
    let src = "fn main() { let q = 0; let m = 0; let y = 0; test.capture([q, m, y]); }";
    let mut program = crush_frontend::parse_source(src).expect("parse");
    let main = program.functions.get_mut("main").expect("main");
    for stmt in main.body.iter_mut() {
        if let Statement::VarDecl { name, value, .. } = stmt {
            *value = match name.as_str() {
                "q" => Expression::AI(AIExpression::Query {
                    query: "capital of nepal".into(),
                    result_type: Some("string".into()),
                    context: HashMap::from([("domain".to_string(), serde_json::json!("geo"))]),
                }),
                "m" => Expression::AI(AIExpression::SemanticMatch {
                    target: Box::new(s("hello there")),
                    concept: "greeting".into(),
                    confidence_threshold: 0.5,
                }),
                "y" => Expression::AI(AIExpression::Synthesize {
                    output_type: CastType::Int,
                    constraints: vec!["sum".into()],
                    context_refs: vec![int(1), int(2)],
                    examples: Some(vec![s("ex")]),
                }),
                _ => continue,
            };
        }
    }
    // A statement-form AI op must not leave its result on the stack.
    main.body.insert(
        0,
        Statement::AI(AIStatement::GoalDeclaration {
            goal_id: "g1".into(),
            description: "test".into(),
            success_criteria: vec![],
            priority: Priority::Low,
            deadline: None,
        }),
    );
    program
}

fn json(v: &Value) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}

fn expected() -> serde_json::Value {
    serde_json::json!([
        {"ok": true, "kind": "query", "echo": [
            {"query": "capital of nepal", "result_type": "string", "context": {"domain": "geo"}}
        ]},
        {"ok": true, "kind": "semantic_match", "echo": [
            {"concept": "greeting", "confidence_threshold": 0.5}, "hello there"
        ]},
        {"ok": true, "kind": "synthesize", "echo": [
            {"output_type": "Int", "constraints": ["sum"], "context_refs": 2}, 1, 2, "ex"
        ]}
    ])
}

/// Records its one argument as JSON (`Value` isn't `Send`; JSON maps sort
/// their keys, so the comparison is order-independent).
struct Capture(Arc<Mutex<serde_json::Value>>);

impl HostCap for Capture {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "test.capture".into(),
            argc: Some(1),
            returns: true,
        }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        *self.0.lock().unwrap() = json(&args[0]);
        // compile.rs only knows builtin caps' `returns`; an unknown cap's
        // call statement is followed by a POP, so return a value.
        Ok(Some(Value::Null))
    }
}

/// Run on the scheduler and on PortableVm; return what each captured.
fn run_both(prog: &crush_vm::Program, ai: bool) -> (serde_json::Value, serde_json::Value) {
    let caps = |slot: &Arc<Mutex<serde_json::Value>>| {
        let mut c = HostCapsBuilder::new().ai_native(ai).build();
        c.register(Box::new(Capture(slot.clone())));
        c
    };
    let a = Arc::new(Mutex::new(serde_json::Value::Null));
    let r = crush_vm::run_with_caps(prog, &Quotas::default(), Some(&caps(&a))).expect("scheduler");
    assert!(
        r.stack.is_empty(),
        "scheduler stack balanced: {:?}",
        r.stack
    );
    let b = Arc::new(Mutex::new(serde_json::Value::Null));
    let mut pvm = PortableVm::new(prog.clone());
    pvm.set_host_caps(caps(&b));
    let r = pvm.run().expect("portable");
    // PortableVm keeps main's implicit `null` return; compare against the
    // same program with no AI ops in it.
    let baseline = {
        let src = "fn main() { let q = 0; let m = 0; let y = 0; test.capture([q, m, y]); }";
        let casm = crush_lang_sdk::compile::compile_crush_to_casm(src).unwrap();
        let mut pvm = PortableVm::new(crush_lang_sdk::compile::casm_to_vm(&casm).unwrap());
        pvm.set_host_caps(caps(&Arc::new(Mutex::new(serde_json::Value::Null))));
        pvm.run().unwrap().stack.len()
    };
    assert_eq!(
        r.stack.len(),
        baseline,
        "PortableVm stack balanced: {:?}",
        r.stack
    );
    let (a, b) = (a.lock().unwrap().clone(), b.lock().unwrap().clone());
    (a, b)
}

#[test]
fn scheduler_and_portable_vm_pass_payload_and_operands() {
    let casm = crush_frontend::compile_cast_owned(program()).expect("compile");
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");
    let (a, b) = run_both(&prog, true);
    assert_eq!(a, expected());
    assert_eq!(b, a, "scheduler and PortableVm agree");
}

#[test]
fn ungranted_ai_ops_are_null_and_keep_the_stack_balanced() {
    let casm = crush_frontend::compile_cast_owned(program()).expect("compile");
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");
    let (a, b) = run_both(&prog, false);
    assert_eq!(a, serde_json::json!([null, null, null]));
    assert_eq!(b, a);
}

#[test]
fn fastvm_resolve_host_request_echoes_the_same_payload() {
    let casm = crush_frontend::compile_cast_owned(program()).expect("compile");
    let caps = HostCapsBuilder::new().ai_native(true).build();
    // The payload FastVM yields is the CASM instruction's args.
    let payload = casm
        .functions
        .values()
        .flat_map(|f| f.body.iter())
        .find(|i| i.op == "ai_query")
        .map(|i| i.args.clone())
        .expect("ai_query instr");
    let req = HostRequest::AiQuery { args: payload };
    let Some(RuntimeValue::String(text)) = resolve_host_request(&req, Some(&caps)) else {
        panic!("granted ai_native.query must resolve to the stub's JSON text");
    };
    let got: serde_json::Value = serde_json::from_str(&text).unwrap();

    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");
    let (cvm1, _) = run_both(&prog, true);
    assert_eq!(got, cvm1[0], "FastVM and CVM1 echo the same query args");

    // Ungranted → unserviced; a stack-operand kind can't be serviced by FastVM.
    assert!(resolve_host_request(&req, Some(&HostCapsBuilder::new().build())).is_none());
    let m = HostRequest::AiSemanticMatch {
        args: serde_json::json!({"stack_args": 1}),
    };
    assert!(resolve_host_request(&m, Some(&caps)).is_none());
}

#[test]
fn fastvm_yields_the_compiled_query_payload() {
    let src = "fn main() { let q = 0; return q; }";
    let mut program = crush_frontend::parse_source(src).expect("parse");
    for stmt in program.functions.get_mut("main").unwrap().body.iter_mut() {
        if let Statement::VarDecl { value, .. } = stmt {
            *value = Expression::AI(AIExpression::Query {
                query: "q?".into(),
                result_type: None,
                context: HashMap::new(),
            });
        }
    }
    let casm = crush_frontend::compile_cast_owned(program).expect("compile");
    match crush_vm::run_fastvm(&casm) {
        Ok(FastYield::Request(HostRequest::AiQuery { args })) => {
            assert_eq!(args["query"], "q?");
        }
        other => panic!("expected an AiQuery request, got {other:?}"),
    }
}
