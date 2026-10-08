//! CRUSH-158: host backends behind `ai_native.query` and
//! `ai_native.agent_delegation`, wired through `HostCapsBuilder`.

use crush_cast::ai::{AIExpression, DelegationStrategy};
use crush_cast::{Expression, Statement};
use crush_lang_sdk::ai_native::providers::{
    AgentStatus, DelegationBackend, QueryProvider, QueryRequest,
};
use crush_lang_sdk::{HostCap, HostCapSpec, HostCapsBuilder};
use crush_vm::vm::{Quotas, Value};
use serde_json::{Value as Json, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Oracle {
    calls: AtomicU32,
}

impl QueryProvider for Oracle {
    fn query(&self, r: &QueryRequest) -> Result<Json, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Json::String(format!("answer to {}", r.query)))
    }
}

#[derive(Default)]
struct Fleet {
    dispatched: Mutex<Vec<String>>,
}

impl DelegationBackend for Fleet {
    fn status(&self, agent: &str) -> Option<AgentStatus> {
        Some(AgentStatus {
            available: agent != "busy",
            rating: 0.5,
        })
    }
    fn dispatch(&self, agent: &str, task: &str) -> Result<String, String> {
        self.dispatched.lock().unwrap().push(agent.to_string());
        Ok(format!("{{\"by\":\"{agent}\",\"task\":\"{task}\"}}"))
    }
}

struct Capture(Arc<Mutex<Json>>);

impl HostCap for Capture {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "test.capture".into(),
            argc: Some(1),
            returns: true,
        }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        *self.0.lock().unwrap() = serde_json::to_value(&args[0]).unwrap();
        Ok(Some(Value::Null))
    }
}

/// `let q = <query>; let d = <delegation>; test.capture([q, d]);`
fn program() -> crush_vm::Program {
    let src = "fn main() { let q = 0; let d = 0; test.capture([q, d]); }";
    let mut program = crush_frontend::parse_source(src).expect("parse");
    for stmt in program.functions.get_mut("main").unwrap().body.iter_mut() {
        if let Statement::VarDecl { name, value, .. } = stmt {
            *value = match name.as_str() {
                "q" => Expression::AI(AIExpression::Query {
                    query: "life".into(),
                    result_type: None,
                    context: HashMap::new(),
                }),
                _ => Expression::AI(AIExpression::AgentDelegation {
                    task: "review".into(),
                    agents: vec!["busy".into(), "free".into()],
                    delegation_strategy: DelegationStrategy::FirstAvailable,
                    expected_format: Some("structured".into()),
                }),
            };
        }
    }
    let casm = crush_frontend::compile_cast_owned(program).expect("compile");
    crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm")
}

fn run(builder: HostCapsBuilder) -> Json {
    let slot = Arc::new(Mutex::new(Json::Null));
    let mut caps = builder.build();
    caps.register(Box::new(Capture(slot.clone())));
    crush_vm::run_with_caps(&program(), &Quotas::default(), Some(&caps)).expect("scheduler");
    slot.lock().unwrap().clone()
}

#[test]
fn compiled_program_reaches_the_host_backends() {
    let oracle = Arc::new(Oracle::default());
    let fleet = Arc::new(Fleet::default());
    let out = run(HostCapsBuilder::new()
        .ai_native(true)
        .query_provider(oracle.clone())
        .delegation_backend(fleet.clone()));
    assert_eq!(
        out[0],
        json!({"ok": true, "kind": "query", "result": "answer to life"})
    );
    assert_eq!(out[1]["agents"], json!(["free"]));
    assert_eq!(out[1]["results"][0]["status"], "done");
    assert_eq!(out[1]["ok"], true);
    assert_eq!(*fleet.dispatched.lock().unwrap(), ["free"]);
}

#[test]
fn a_backend_is_not_a_grant() {
    let oracle = Arc::new(Oracle::default());
    let fleet = Arc::new(Fleet::default());
    let out = run(HostCapsBuilder::new()
        .query_provider(oracle.clone())
        .delegation_backend(fleet.clone()));
    assert_eq!(
        out,
        json!([null, null]),
        "without ai_native(true) the gates are absent"
    );
    assert_eq!(oracle.calls.load(Ordering::SeqCst), 0);
    assert!(fleet.dispatched.lock().unwrap().is_empty());
}

#[test]
fn without_backends_the_echo_stubs_remain() {
    let out = run(HostCapsBuilder::new().ai_native(true));
    assert_eq!(out[0]["echo"][0]["query"], "life");
    assert_eq!(out[1]["kind"], "agent_delegation");
    assert!(out[1].get("echo").is_some());
}

#[test]
fn toolchain_steps_see_the_backends() {
    let oracle = Arc::new(Oracle::default());
    let caps = HostCapsBuilder::new()
        .ai_native(true)
        .query_provider(oracle.clone())
        .build();
    let payload: Value = serde_json::from_value(json!({
        "tools": [{"tool_name": "ai_native.query", "parameters": {"args": [{"query": "x"}]}}]
    }))
    .unwrap();
    let out = caps
        .get("ai_native.toolchain")
        .unwrap()
        .call(vec![payload])
        .unwrap()
        .unwrap();
    let out = serde_json::to_value(out).unwrap();
    assert_eq!(out["results"][0]["value"]["result"], "answer to x");
    assert_eq!(oracle.calls.load(Ordering::SeqCst), 1);
}
