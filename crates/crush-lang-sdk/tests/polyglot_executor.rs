//! CRUSH-271: an embedder supplies the runner for `@lang` blocks.
//!
//! The VM keeps the syntax, the marshaling and the `polyglot.<lang>` grant
//! check; a [`PolyglotExecutor`] on the host caps decides how a granted block
//! runs, on both engines. None of these tests needs an interpreter on the
//! host: the executor below never spawns anything.

use std::sync::{Arc, Mutex};

use crush_lang_sdk::{
    HostCaps, HostCapsBuilder, PolyglotError, PolyglotExecutor, PolyglotRequest, Runtime,
    polyglot_executor_fn,
};
use crush_vm::portable_vm::PortableVm;
use crush_vm::scheduler::CRUSH_RESULT_SENTINEL;
use crush_vm::{Program, Quotas, VmError};

/// What one call to the executor saw.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    lang: String,
    vars: Vec<(String, String)>,
    outputs: Vec<String>,
    deps: Vec<String>,
    gate: String,
    deadline_ms: u64,
    code_mentions_body: bool,
}

/// A fake runner: doubles the JSON integer input `n` into every declared
/// output, prints a line of its own, and records each request.
#[derive(Default)]
struct Doubler {
    seen: Mutex<Vec<Seen>>,
}

impl PolyglotExecutor for Doubler {
    fn execute(&self, req: &PolyglotRequest<'_>) -> Result<String, PolyglotError> {
        self.seen.lock().unwrap().push(Seen {
            lang: req.lang.to_string(),
            vars: req.vars.to_vec(),
            outputs: req.outputs.to_vec(),
            deps: req.deps.to_vec(),
            gate: req.gate.to_string(),
            deadline_ms: req.deadline_ms,
            code_mentions_body: req.code.contains("n * 2"),
        });
        Ok(doubled(req.lang, req.vars, req.outputs))
    }
}

/// The fake runner's output: doubles the JSON integer input `n` into every
/// declared output, after a line of its own.
fn doubled(lang: &str, vars: &[(String, String)], outputs: &[String]) -> String {
    let n: i64 = vars
        .iter()
        .find(|(name, _)| name == "n")
        .map(|(_, v)| v.parse().expect("n is JSON-encoded"))
        .unwrap_or(0);
    let outs: serde_json::Map<String, serde_json::Value> =
        outputs.iter().map(|o| (o.clone(), serde_json::json!(n * 2))).collect();
    format!("ran {lang} in the embedder\n{CRUSH_RESULT_SENTINEL}{}\n", serde_json::Value::Object(outs))
}

fn compile(src: &str) -> Program {
    crush_lang_sdk::compile::compile_crush_source(src).expect("compile")
}

fn granted_with(executor: Arc<dyn PolyglotExecutor>) -> HostCaps {
    let mut caps = HostCaps::new();
    caps.grant_polyglot(&["python"]);
    caps.set_polyglot_executor(executor);
    caps
}

fn scheduler(program: &Program, caps: &HostCaps) -> Result<String, VmError> {
    crush_vm::run_with_caps(program, &Quotas::default(), Some(caps)).map(|r| r.output)
}

fn portable(program: &Program, caps: &HostCaps) -> Result<String, VmError> {
    let mut vm = PortableVm::new(program.clone());
    vm.set_host_caps(caps.clone());
    vm.run().map(|r| r.output)
}

/// The block's visible output is trimmed (as on the subprocess path), so the
/// block's own line and the next `print` run together.
const EXPECTED: &str = "ran python in the embedder42\n";

const DOUBLE: &str = "fn main() {\n  let n = 21\n  @python -> y {\n    y = n * 2\n  }\n  print(y)\n}\n";

#[test]
fn the_executor_runs_the_block_on_the_scheduler_vm() {
    let doubler = Arc::new(Doubler::default());
    let out = scheduler(&compile(DOUBLE), &granted_with(doubler.clone())).expect("run");
    assert_eq!(out, EXPECTED);

    let seen = doubler.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one block, one call");
    let call = &seen[0];
    assert_eq!(call.lang, "python");
    assert_eq!(call.vars, vec![("n".to_string(), "21".to_string())]);
    assert_eq!(call.outputs, vec!["y".to_string()]);
    assert!(call.deps.is_empty());
    assert_eq!(call.gate, "polyglot.python");
    assert_eq!(call.deadline_ms, Quotas::default().max_wall_time_ms);
    assert!(call.code_mentions_body, "the executor gets the block's source");
}

#[test]
fn both_engines_agree_with_the_same_executor() {
    let program = compile(DOUBLE);
    let doubler = Arc::new(Doubler::default());
    let caps = granted_with(doubler.clone());
    let native = scheduler(&program, &caps).expect("scheduler");
    let port = portable(&program, &caps).expect("portable");
    assert_eq!(native, port);
    let seen = doubler.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], seen[1], "both engines hand the executor the same request");
}

#[test]
fn the_executor_matches_portable_vms_lang_executor() {
    // The older `PortableVm::set_lang_executor` seam, given the same runner,
    // produces what the scheduler VM produces through the new one.
    let program = compile(DOUBLE);
    let native = scheduler(&program, &granted_with(Arc::new(Doubler::default()))).expect("scheduler");

    let mut vm = PortableVm::new(program);
    let mut caps = HostCaps::new();
    caps.grant_polyglot(&["python"]);
    vm.set_host_caps(caps);
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    vm.set_lang_executor(Box::new(move |lang, _code, env| {
        *counter.lock().unwrap() += 1;
        Ok(doubled(lang, env, &["y".to_string()]))
    }));
    let legacy = vm.run().map(|r| r.output).expect("portable legacy");
    assert_eq!(native, legacy);
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[test]
fn an_ungranted_block_never_reaches_the_executor() {
    let program = compile(DOUBLE);
    let doubler = Arc::new(Doubler::default());

    // 1. No `polyglot.python` grant at all.
    let mut no_grant = HostCaps::new();
    no_grant.set_polyglot_executor(doubler.clone());
    for result in [scheduler(&program, &no_grant), portable(&program, &no_grant)] {
        let e = result.expect_err("refused").to_string();
        assert!(e.contains("requires the 'polyglot.python' capability"), "{e}");
    }

    // 2. Granted, but outside the embedder's `allowed_caps`.
    let caps = granted_with(doubler.clone());
    let quotas = Quotas { allowed_caps: Some(vec!["io.print".into()]), ..Quotas::default() };
    match crush_vm::run_with_caps(&program, &quotas, Some(&caps)) {
        Err(VmError::CapDenied(cap)) => assert_eq!(cap, "polyglot.python"),
        other => panic!("expected CapDenied, got {other:?}"),
    }

    // 3. Granted, but a language outside the allowlist.
    let zsh = compile("fn main() {\n  @zsh { echo hi }\n}\n");
    let mut every = HostCaps::new();
    every.grant_polyglot(&["python", "javascript", "bash", "zsh"]);
    every.set_polyglot_executor(doubler.clone());
    let e = scheduler(&zsh, &every).expect_err("refused").to_string();
    assert!(e.contains("no executor registered for language 'zsh'"), "{e}");

    assert!(doubler.seen.lock().unwrap().is_empty(), "the executor was called for an ungranted block");
}

#[test]
fn an_executor_error_becomes_a_vm_error() {
    let program = compile(DOUBLE);
    let failing = polyglot_executor_fn(|_req: &PolyglotRequest<'_>| {
        Err(PolyglotError::Failed("capsule host is offline".into()))
    });
    let caps = granted_with(failing);
    for result in [scheduler(&program, &caps), portable(&program, &caps)] {
        let e = result.expect_err("fails").to_string();
        assert!(e.contains("@python block failed"), "{e}");
        assert!(e.contains("capsule host is offline"), "{e}");
    }

    let guest = granted_with(polyglot_executor_fn(|_req: &PolyglotRequest<'_>| {
        Err(PolyglotError::Guest("ZeroDivisionError: division by zero".into()))
    }));
    match crush_vm::run_with_caps(&program, &Quotas::default(), Some(&guest)) {
        Err(VmError::LangRuntimeError { lang, message, .. }) => {
            assert_eq!(lang, "python");
            assert!(message.contains("ZeroDivisionError"), "{message}");
        }
        other => panic!("expected LangRuntimeError, got {other:?}"),
    }

    let slow = granted_with(polyglot_executor_fn(|_req: &PolyglotRequest<'_>| Err(PolyglotError::Timeout)));
    let quotas = Quotas { max_wall_time_ms: 250, ..Quotas::default() };
    match crush_vm::run_with_caps(&program, &quotas, Some(&slow)) {
        Err(VmError::CapTimeout { cap, limit_ms }) => {
            assert_eq!(cap, "polyglot.python");
            assert_eq!(limit_ms, 250);
        }
        other => panic!("expected CapTimeout, got {other:?}"),
    }
}

#[test]
fn spawned_tasks_use_the_same_executor() {
    let src = "fn work(n) {\n  @python -> y {\n    y = n * 2\n  }\n  return y\n}\nfn main() {\n  let a = spawn work(5)\n  let b = spawn work(7)\n  yield\n  print(await a)\n  print(await b)\n}\n";
    let program = compile(src);
    let doubler = Arc::new(Doubler::default());
    let out = scheduler(&program, &granted_with(doubler.clone())).expect("run");
    assert!(out.ends_with("10\n14\n"), "{out:?}");
    let seen = doubler.seen.lock().unwrap();
    let mut ns: Vec<&str> = seen.iter().map(|s| s.vars[0].1.as_str()).collect();
    ns.sort();
    assert_eq!(ns, vec!["5", "7"], "each spawned task's block went through the executor");
}

#[test]
fn parallel_map_items_get_no_grant_and_so_no_executor() {
    // A parallel_map item runs in a fresh share-nothing VM with no host
    // capabilities — no polyglot grant, so its block is refused before any
    // executor could be asked, on both engines, exactly as without one.
    let src = "fn work(n) {\n  @python -> y {\n    y = n * 2\n  }\n  return y\n}\nfn main() {\n  print(parallel_map([1, 2, 3], work))\n}\n";
    let program = compile(src);
    let doubler = Arc::new(Doubler::default());
    let caps = granted_with(doubler.clone());
    for result in [scheduler(&program, &caps), portable(&program, &caps)] {
        let e = result.expect_err("refused").to_string();
        assert!(e.contains("polyglot.python"), "{e}");
    }
    assert!(doubler.seen.lock().unwrap().is_empty());
}

#[test]
fn runtime_and_builder_carry_the_executor() {
    let program = compile(DOUBLE);

    // Through the SDK builder.
    let doubler = Arc::new(Doubler::default());
    let caps = HostCapsBuilder::new().polyglot(&["python"]).polyglot_executor(doubler.clone()).build();
    let out = Runtime::new().with_host_caps(caps).run(&program).expect("run").output;
    assert_eq!(out, EXPECTED);

    // Through `Runtime::with_polyglot_executor`, before or after the caps.
    for executor_first in [true, false] {
        let doubler = Arc::new(Doubler::default());
        let caps = HostCapsBuilder::new().polyglot(&["python"]).build();
        let rt = if executor_first {
            Runtime::new().with_polyglot_executor(doubler.clone()).with_host_caps(caps)
        } else {
            Runtime::new().with_host_caps(caps).with_polyglot_executor(doubler.clone())
        };
        assert_eq!(rt.run(&program).expect("run").output, EXPECTED);
        assert_eq!(doubler.seen.lock().unwrap().len(), 1);
    }

    // Without a grant the Runtime refuses, executor or not.
    let doubler = Arc::new(Doubler::default());
    let rt = Runtime::new().with_polyglot_executor(doubler.clone());
    assert!(rt.run(&program).is_err());
    assert!(doubler.seen.lock().unwrap().is_empty());
}
