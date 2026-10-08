//! CRUSH-157: a compiled `ai_native.toolchain` expression runs its tools
//! through the program's own grants, on the scheduler and on PortableVm.

use crush_cast::ai::{AIExpression, ErrorHandling, ExecutionStrategy, ToolCall};
use crush_cast::{Expression, Statement};
use crush_lang_sdk::{HostCap, HostCapSpec, HostCaps, HostCapsBuilder};
use crush_vm::portable_vm::PortableVm;
use crush_vm::vm::{Quotas, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct Double;

impl HostCap for Double {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "test.double".into(),
            argc: Some(1),
            returns: true,
        }
    }
    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        match args[0] {
            Value::Int(n) => Ok(Some(Value::Int(n * 2))),
            ref other => Err(format!("not an int: {other:?}")),
        }
    }
}

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
        *self.0.lock().unwrap() = serde_json::to_value(&args[0]).unwrap();
        Ok(Some(Value::Null))
    }
}

fn tool(name: &str, args: serde_json::Value, binding: Option<&str>) -> ToolCall {
    ToolCall {
        tool_name: name.into(),
        parameters: HashMap::from([("args".to_string(), args)]),
        result_binding: binding.map(String::from),
        condition: None,
        required_capability: None,
    }
}

/// `let r = <tool chain>; test.capture(r);`
fn program() -> crush_vm::Program {
    let src = "fn main() { let r = 0; test.capture(r); }";
    let mut program = crush_frontend::parse_source(src).expect("parse");
    for stmt in program.functions.get_mut("main").unwrap().body.iter_mut() {
        if let Statement::VarDecl { value, .. } = stmt {
            *value = Expression::AI(AIExpression::ToolChain {
                tools: vec![
                    tool("test.double", serde_json::json!([21]), Some("x")),
                    // Not granted: must fail its step without running.
                    tool("fs.write", serde_json::json!(["pwned.txt", "x"]), None),
                    tool("test.double", serde_json::json!(["$x"]), None),
                ],
                strategy: ExecutionStrategy::Sequential,
                error_handling: ErrorHandling::ContinueOnError,
            });
        }
    }
    let casm = crush_frontend::compile_cast_owned(program).expect("compile");
    crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm")
}

fn caps(slot: &Arc<Mutex<serde_json::Value>>) -> HostCaps {
    let mut caps = HostCapsBuilder::new().build();
    caps.register(Box::new(Double));
    caps.register(Box::new(Capture(slot.clone())));
    // Registered last, so its snapshot sees the tools above.
    crush_lang_sdk::ai_native::register(&mut caps);
    caps
}

#[test]
fn compiled_tool_chain_runs_granted_tools_and_refuses_the_rest() {
    let prog = program();
    let a = Arc::new(Mutex::new(serde_json::Value::Null));
    crush_vm::run_with_caps(&prog, &Quotas::default(), Some(&caps(&a))).expect("scheduler");
    let b = Arc::new(Mutex::new(serde_json::Value::Null));
    let mut pvm = PortableVm::new(prog);
    pvm.set_host_caps(caps(&b));
    pvm.run().expect("portable");

    let out = a.lock().unwrap().clone();
    assert_eq!(out["aborted"], false);
    let r = &out["results"];
    assert_eq!(r[0]["value"], 42);
    assert_eq!(r[1]["ok"], false);
    assert!(
        r[1]["error"]
            .as_str()
            .unwrap()
            .contains("`fs.write` is not granted"),
        "{out}"
    );
    assert_eq!(r[2]["value"], 84, "the binding fed the next step");
    assert!(!std::path::Path::new("pwned.txt").exists());
    assert_eq!(*b.lock().unwrap(), out, "PortableVm agrees");
}

#[test]
fn builder_ai_native_installs_the_engine_not_the_echo_stub() {
    let caps = HostCapsBuilder::new().ai_native(true).build();
    let tc = caps.get("ai_native.toolchain").expect("registered");
    let payload: Value = serde_json::from_value(serde_json::json!({"tools": []})).unwrap();
    let out = serde_json::to_value(tc.call(vec![payload]).unwrap().unwrap()).unwrap();
    assert_eq!(
        out,
        serde_json::json!({"results": [], "aborted": false, "abort_reason": null})
    );
}
