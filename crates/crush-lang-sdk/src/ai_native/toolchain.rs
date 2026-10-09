//! CRUSH-157: the `ai_native.toolchain` strategy engine.
//!
//! Runs a compiled tool list (`crush_vm::ai_args` payload: `{tools, strategy,
//! error_handling}`) and returns `{results, aborted, abort_reason}`. Every
//! step is dispatched through a [`HostCaps`] snapshot taken when the cap was
//! built, so grants apply per tool: a tool whose capability isn't registered
//! there — or whose `required_capability` isn't — fails its step and never
//! runs. No backends live here; the tools are whatever the host granted.
//!
//! Strategies: `sequential`, `parallel` (scoped threads), `conditional`
//! (tool *i* also gated by `conditions[i]`), `retry` (each step attempted up
//! to `max_attempts` times). Error policies, applied after a step's attempts
//! are spent: `fail_fast`, `continue_on_error`, `retry` (`max_retries` more
//! attempts, optionally only when the error contains `retry_condition`, then
//! abort), `fallback` (try `fallback_tools` in order, then abort). A grant
//! refusal is never retried — it can't succeed on a second attempt — but a
//! fallback may replace it.
//!
//! Tool arguments: `parameters.args` (an array) is passed positionally; any
//! other non-empty `parameters` object is passed as one map argument; empty
//! parameters pass no arguments. A string parameter `"$name"` is replaced by
//! the value bound to `result_binding: "name"` by an earlier step.
//! Conditions name a binding (`"name"` runs iff it is bound and truthy,
//! `"!name"` iff not); an empty condition always runs.
//!
//! Values cross into tools as JSON (`Value` isn't `Send`, and parallel steps
//! run on other threads); every strategy uses the same path so a chain's
//! result doesn't depend on how it was scheduled.

use crush_vm::host::HostCapError;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use serde_json::{Map, Value as Json, json};
use std::collections::HashMap;
use std::sync::Arc;

/// The real `ai_native.toolchain` capability.
pub struct ToolchainCap {
    tools: Arc<HostCaps>,
}

impl ToolchainCap {
    /// `tools` is the registry steps are dispatched through — usually a
    /// clone of the program's own `HostCaps`, taken once every other grant is
    /// registered (`HostCaps` clones share handler instances).
    pub fn new(tools: Arc<HostCaps>) -> Self {
        Self { tools }
    }
}

impl HostCap for ToolchainCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "ai_native.toolchain".into(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        self.run(args, crush_vm::Quotas::default().max_wall_time_ms)
    }

    fn call_with_deadline(
        &self,
        args: Vec<Value>,
        deadline_ms: u64,
    ) -> Result<Option<Value>, HostCapError> {
        self.run(args, deadline_ms).map_err(HostCapError::Message)
    }
}

impl ToolchainCap {
    fn run(&self, args: Vec<Value>, deadline_ms: u64) -> Result<Option<Value>, String> {
        let payload = match args.first() {
            Some(v) => serde_json::to_value(v).map_err(|e| e.to_string())?,
            None => return Err("ai_native.toolchain: missing payload".into()),
        };
        let chain = Chain::parse(&payload)?;
        let out = Engine {
            tools: &self.tools,
            deadline_ms,
        }
        .run(&chain);
        serde_json::from_value(out)
            .map(Some)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone)]
struct Tool {
    name: String,
    parameters: Map<String, Json>,
    result_binding: Option<String>,
    condition: Option<String>,
    required_capability: Option<String>,
}

impl Tool {
    fn parse(v: &Json) -> Result<Self, String> {
        let obj = v
            .as_object()
            .ok_or("ai_native.toolchain: a tool must be an object")?;
        let text = |k: &str| obj.get(k).and_then(Json::as_str).map(String::from);
        Ok(Tool {
            name: text("tool_name")
                .filter(|n| !n.is_empty())
                .ok_or("ai_native.toolchain: tool without tool_name")?,
            parameters: obj
                .get("parameters")
                .and_then(Json::as_object)
                .cloned()
                .unwrap_or_default(),
            result_binding: text("result_binding"),
            condition: text("condition"),
            required_capability: text("required_capability"),
        })
    }
}

fn parse_tools(v: Option<&Json>) -> Result<Vec<Tool>, String> {
    match v {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Array(a)) => a.iter().map(Tool::parse).collect(),
        Some(_) => Err("ai_native.toolchain: tools must be an array".into()),
    }
}

#[derive(Debug)]
enum Strategy {
    Sequential,
    Parallel,
    Conditional(Vec<String>),
    Retry(u32),
}

#[derive(Debug)]
enum Policy {
    FailFast,
    Continue,
    Retry { max: u32, only_if: Option<String> },
    Fallback(Vec<Tool>),
}

struct Chain {
    tools: Vec<Tool>,
    strategy: Strategy,
    policy: Policy,
}

fn kind(v: Option<&Json>) -> &str {
    v.and_then(|s| s.get("type"))
        .and_then(Json::as_str)
        .unwrap_or("")
}

fn count(v: Option<&Json>, key: &str) -> u32 {
    v.and_then(|s| s.get(key))
        .and_then(Json::as_u64)
        .unwrap_or(1)
        .min(u32::MAX as u64) as u32
}

impl Chain {
    fn parse(p: &Json) -> Result<Self, String> {
        let s = p.get("strategy");
        let strategy = match kind(s) {
            "" | "sequential" => Strategy::Sequential,
            "parallel" => Strategy::Parallel,
            "conditional" => Strategy::Conditional(
                s.and_then(|s| s.get("conditions"))
                    .and_then(Json::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|c| c.as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "retry" => Strategy::Retry(count(s, "max_attempts").max(1)),
            other => return Err(format!("ai_native.toolchain: unknown strategy `{other}`")),
        };
        let e = p.get("error_handling");
        let policy = match kind(e) {
            "" | "fail_fast" => Policy::FailFast,
            "continue_on_error" => Policy::Continue,
            "retry" => Policy::Retry {
                max: count(e, "max_retries"),
                only_if: e
                    .and_then(|e| e.get("retry_condition"))
                    .and_then(Json::as_str)
                    .map(String::from),
            },
            "fallback" => Policy::Fallback(parse_tools(e.and_then(|e| e.get("fallback_tools")))?),
            other => {
                return Err(format!(
                    "ai_native.toolchain: unknown error_handling `{other}`"
                ));
            }
        };
        Ok(Chain {
            tools: parse_tools(p.get("tools"))?,
            strategy,
            policy,
        })
    }
}

/// Why a single call failed.
#[derive(Debug, Clone)]
enum Failure {
    /// Not granted — deterministic, never retried.
    Denied(String),
    Error(String),
}

impl Failure {
    fn message(&self) -> &str {
        match self {
            Failure::Denied(m) | Failure::Error(m) => m,
        }
    }
}

/// The record of one step.
struct Step {
    tool: String,
    outcome: Result<Json, Failure>,
    attempts: u32,
    skipped: bool,
    fallback: Option<String>,
}

impl Step {
    fn to_json(&self) -> Json {
        let (ok, value, error) = match &self.outcome {
            Ok(v) => (true, v.clone(), Json::Null),
            Err(f) => (false, Json::Null, Json::String(f.message().to_string())),
        };
        let mut m = json!({
            "tool": self.tool, "ok": ok, "value": value, "error": error,
            "attempts": self.attempts, "skipped": self.skipped,
        });
        if let Some(f) = &self.fallback {
            m["fallback"] = Json::String(f.clone());
        }
        m
    }
}

struct Engine<'a> {
    tools: &'a HostCaps,
    deadline_ms: u64,
}

impl Engine<'_> {
    fn run(&self, chain: &Chain) -> Json {
        let mut bindings: HashMap<String, Json> = HashMap::new();
        let mut steps: Vec<Step> = Vec::new();
        let mut abort: Option<String> = None;
        let attempts = match chain.strategy {
            Strategy::Retry(n) => n,
            _ => 1,
        };

        if let Strategy::Parallel = chain.strategy {
            // Every step runs (concurrently) before the error policy is
            // applied, in tool order. Conditions see no bindings.
            let first: Vec<Option<(Result<Json, Failure>, u32)>> = std::thread::scope(|s| {
                let handles: Vec<_> = chain
                    .tools
                    .iter()
                    .map(|t| {
                        let run = self.gate(t, None, &bindings);
                        s.spawn(move || run.then(|| self.attempt(t, &HashMap::new(), attempts)))
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join().unwrap_or_else(|_| {
                            Some((Err(Failure::Error("tool panicked".into())), 1))
                        })
                    })
                    .collect()
            });
            for (tool, r) in chain.tools.iter().zip(first) {
                let step = match r {
                    None => skipped(tool),
                    Some((outcome, n)) => self.settle(tool, outcome, n, &chain.policy, &bindings),
                };
                if abort.is_none()
                    && let Err(f) = &step.outcome
                    && !matches!(chain.policy, Policy::Continue)
                {
                    abort = Some(format!("{}: {}", tool.name, f.message()));
                }
                bind(&mut bindings, tool, &step);
                steps.push(step);
            }
        } else {
            for (i, tool) in chain.tools.iter().enumerate() {
                let extra = match &chain.strategy {
                    Strategy::Conditional(c) => c.get(i).map(String::as_str),
                    _ => None,
                };
                if !self.gate(tool, extra, &bindings) {
                    steps.push(skipped(tool));
                    continue;
                }
                let (outcome, n) = self.attempt(tool, &bindings, attempts);
                let step = self.settle(tool, outcome, n, &chain.policy, &bindings);
                let failed = step.outcome.as_ref().err().map(|f| f.message().to_string());
                bind(&mut bindings, tool, &step);
                steps.push(step);
                if let Some(msg) = failed
                    && !matches!(chain.policy, Policy::Continue)
                {
                    abort = Some(format!("{}: {msg}", tool.name));
                    break;
                }
            }
        }

        json!({
            "results": steps.iter().map(Step::to_json).collect::<Vec<_>>(),
            "aborted": abort.is_some(),
            "abort_reason": abort,
        })
    }

    /// Should `tool` run? Its own condition and, under `conditional`, the
    /// strategy's condition for its position must both hold.
    fn gate(&self, tool: &Tool, extra: Option<&str>, bindings: &HashMap<String, Json>) -> bool {
        [tool.condition.as_deref(), extra]
            .into_iter()
            .flatten()
            .all(|c| holds(c, bindings))
    }

    /// Up to `n` attempts; stops early on success or a grant refusal.
    fn attempt(
        &self,
        tool: &Tool,
        bindings: &HashMap<String, Json>,
        n: u32,
    ) -> (Result<Json, Failure>, u32) {
        let mut last = Err(Failure::Error("not attempted".into()));
        for i in 1..=n.max(1) {
            last = self.call(tool, bindings);
            if !matches!(last, Err(Failure::Error(_))) {
                return (last, i);
            }
        }
        (last, n.max(1))
    }

    /// Apply the error policy to a failed step (retry / fallback); a step that
    /// is still failing afterwards is left failed for the caller to act on.
    fn settle(
        &self,
        tool: &Tool,
        outcome: Result<Json, Failure>,
        attempts: u32,
        policy: &Policy,
        bindings: &HashMap<String, Json>,
    ) -> Step {
        let mut step = Step {
            tool: tool.name.clone(),
            outcome,
            attempts,
            skipped: false,
            fallback: None,
        };
        let Err(failure) = &step.outcome else {
            return step;
        };
        match policy {
            Policy::Retry { max, only_if } => {
                let retryable = matches!(failure, Failure::Error(m)
                    if only_if.as_deref().is_none_or(|c| m.contains(c)));
                if retryable {
                    let (outcome, n) = self.attempt(tool, bindings, *max);
                    step.outcome = outcome;
                    step.attempts += n;
                }
            }
            Policy::Fallback(alternatives) => {
                for alt in alternatives {
                    let (outcome, n) = self.attempt(alt, bindings, 1);
                    step.attempts += n;
                    if outcome.is_ok() {
                        step.outcome = outcome;
                        step.fallback = Some(alt.name.clone());
                        break;
                    }
                }
            }
            Policy::FailFast | Policy::Continue => {}
        }
        step
    }

    /// One grant-checked call of `tool` through the registry.
    fn call(&self, tool: &Tool, bindings: &HashMap<String, Json>) -> Result<Json, Failure> {
        if let Some(req) = &tool.required_capability
            && self.tools.get(req).is_none()
        {
            return Err(Failure::Denied(format!(
                "requires capability `{req}`, which is not granted"
            )));
        }
        let Some(cap) = self.tools.get(&tool.name) else {
            return Err(Failure::Denied(format!(
                "capability `{}` is not granted to this toolchain",
                tool.name
            )));
        };
        let args = tool_args(&tool.parameters, bindings)?;
        let spec = cap.spec();
        if let Some(n) = spec.argc
            && n != args.len()
        {
            return Err(Failure::Error(format!(
                "expects {n} argument(s), got {}",
                args.len()
            )));
        }
        match cap.call_with_deadline(args, self.deadline_ms) {
            Ok(v) => serde_json::to_value(v.unwrap_or(Value::Null))
                .map_err(|e| Failure::Error(e.to_string())),
            Err(HostCapError::Timeout) => Err(Failure::Error(format!(
                "timed out after {} ms",
                self.deadline_ms
            ))),
            Err(HostCapError::Message(m)) => Err(Failure::Error(m)),
            // A tool step can't end the whole program.
            Err(HostCapError::Exit(code)) => {
                Err(Failure::Error(format!("tried to exit with status {code}")))
            }
        }
    }
}

fn skipped(tool: &Tool) -> Step {
    Step {
        tool: tool.name.clone(),
        outcome: Ok(Json::Null),
        attempts: 0,
        skipped: true,
        fallback: None,
    }
}

fn bind(bindings: &mut HashMap<String, Json>, tool: &Tool, step: &Step) {
    if let (Some(name), Ok(v), false) = (&tool.result_binding, &step.outcome, step.skipped) {
        bindings.insert(name.clone(), v.clone());
    }
}

fn holds(condition: &str, bindings: &HashMap<String, Json>) -> bool {
    let c = condition.trim();
    let (negate, name) = match c.strip_prefix('!') {
        Some(rest) => (true, rest.trim()),
        None => (false, c),
    };
    if name.is_empty() {
        return true;
    }
    let truthy = bindings
        .get(name)
        .and_then(|j| serde_json::from_value::<Value>(j.clone()).ok())
        .is_some_and(|v| v.is_truthy());
    truthy != negate
}

fn tool_args(
    parameters: &Map<String, Json>,
    bindings: &HashMap<String, Json>,
) -> Result<Vec<Value>, Failure> {
    let resolve = |j: &Json| -> Json {
        match j
            .as_str()
            .and_then(|s| s.strip_prefix('$'))
            .and_then(|n| bindings.get(n))
        {
            Some(bound) => bound.clone(),
            None => j.clone(),
        }
    };
    let to_value =
        |j: Json| serde_json::from_value::<Value>(j).map_err(|e| Failure::Error(e.to_string()));
    if let Some(Json::Array(positional)) = parameters.get("args") {
        return positional.iter().map(|j| to_value(resolve(j))).collect();
    }
    if parameters.is_empty() {
        return Ok(Vec::new());
    }
    let map: Map<String, Json> = parameters
        .iter()
        .map(|(k, v)| (k.clone(), resolve(v)))
        .collect();
    Ok(vec![to_value(Json::Object(map))?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fake tool: fails its first `fail_first` calls, then returns
    /// `"<name>:<args as JSON>"`. Records every call.
    struct Fake {
        name: &'static str,
        fail_first: u32,
        calls: Arc<AtomicU32>,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl HostCap for Fake {
        fn spec(&self) -> HostCapSpec {
            HostCapSpec {
                name: self.name.into(),
                argc: None,
                returns: true,
            }
        }
        fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.log.lock().unwrap().push(self.name.to_string());
            if n <= self.fail_first {
                return Err(format!("{} transient failure {n}", self.name));
            }
            Ok(Some(Value::Str(format!(
                "{}:{}",
                self.name,
                serde_json::to_string(&args).unwrap()
            ))))
        }
    }

    struct World {
        caps: HostCaps,
        calls: HashMap<&'static str, Arc<AtomicU32>>,
        log: Arc<Mutex<Vec<String>>>,
    }

    /// `ok` and `ok2` always succeed, `bad` always fails, `flaky` fails once.
    fn world() -> World {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut caps = HostCaps::new();
        let mut calls = HashMap::new();
        for (name, fail_first) in [("ok", 0), ("ok2", 0), ("bad", u32::MAX), ("flaky", 1)] {
            let c = Arc::new(AtomicU32::new(0));
            calls.insert(name, c.clone());
            caps.register(Box::new(Fake {
                name,
                fail_first,
                calls: c,
                log: log.clone(),
            }));
        }
        World { caps, calls, log }
    }

    impl World {
        fn run(&self, payload: Json) -> Json {
            let cap = ToolchainCap::new(Arc::new(self.caps.clone()));
            let arg: Value = serde_json::from_value(payload).unwrap();
            serde_json::to_value(cap.call(vec![arg]).unwrap().unwrap()).unwrap()
        }
        fn calls(&self, name: &str) -> u32 {
            self.calls[name].load(Ordering::SeqCst)
        }
    }

    fn tool(name: &str) -> Json {
        json!({"tool_name": name, "parameters": {}})
    }

    fn chain(tools: Vec<Json>, strategy: Json, error_handling: Json) -> Json {
        json!({"tools": tools, "strategy": strategy, "error_handling": error_handling})
    }

    fn oks(out: &Json) -> Vec<bool> {
        out["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["ok"].as_bool().unwrap())
            .collect()
    }

    const STRATEGIES: [&str; 4] = ["sequential", "parallel", "conditional", "retry"];

    fn strategy(s: &str) -> Json {
        match s {
            "conditional" => json!({"type": "conditional", "conditions": []}),
            "retry" => json!({"type": "retry", "max_attempts": 1, "backoff": "Fixed"}),
            other => json!({"type": other}),
        }
    }

    #[test]
    fn all_succeed_under_every_strategy() {
        for s in STRATEGIES {
            let w = world();
            let out = w.run(chain(
                vec![tool("ok"), tool("ok2")],
                strategy(s),
                json!({"type": "fail_fast"}),
            ));
            assert_eq!(oks(&out), [true, true], "{s}");
            assert_eq!(out["aborted"], false, "{s}");
            assert_eq!(out["abort_reason"], Json::Null, "{s}");
            assert_eq!(out["results"][0]["value"], "ok:[]", "{s}");
        }
    }

    #[test]
    fn fail_fast_aborts_on_the_first_failure() {
        for s in STRATEGIES {
            let w = world();
            let out = w.run(chain(
                vec![tool("ok"), tool("bad"), tool("ok2")],
                strategy(s),
                json!({"type": "fail_fast"}),
            ));
            assert_eq!(out["aborted"], true, "{s}");
            assert!(
                out["abort_reason"].as_str().unwrap().starts_with("bad: "),
                "{s}: {out}"
            );
            if s == "parallel" {
                // Parallel already ran everything; the policy reports, it can't un-run.
                assert_eq!(oks(&out), [true, false, true]);
            } else {
                assert_eq!(oks(&out), [true, false], "{s}: later steps don't run");
                assert_eq!(w.calls("ok2"), 0, "{s}");
            }
        }
    }

    #[test]
    fn continue_on_error_runs_everything() {
        for s in STRATEGIES {
            let w = world();
            let out = w.run(chain(
                vec![tool("bad"), tool("ok")],
                strategy(s),
                json!({"type": "continue_on_error"}),
            ));
            assert_eq!(oks(&out), [false, true], "{s}");
            assert_eq!(out["aborted"], false, "{s}");
        }
    }

    #[test]
    fn retry_policy_succeeds_on_attempt_two() {
        for s in STRATEGIES {
            let w = world();
            let policy = json!({"type": "retry", "max_retries": 2, "retry_condition": null});
            let out = w.run(chain(vec![tool("flaky"), tool("ok")], strategy(s), policy));
            assert_eq!(oks(&out), [true, true], "{s}: {out}");
            assert_eq!(out["results"][0]["attempts"], 2, "{s}");
            assert_eq!(w.calls("flaky"), 2, "{s}");
            assert_eq!(out["aborted"], false, "{s}");
        }
    }

    #[test]
    fn retry_policy_gives_up_then_aborts_and_honours_retry_condition() {
        for s in STRATEGIES {
            let w = world();
            let policy = json!({"type": "retry", "max_retries": 2});
            let out = w.run(chain(vec![tool("bad")], strategy(s), policy));
            assert_eq!(out["results"][0]["attempts"], 3, "{s}");
            assert_eq!(out["aborted"], true, "{s}");

            let w = world();
            let policy = json!({"type": "retry", "max_retries": 2, "retry_condition": "timeout"});
            let out = w.run(chain(vec![tool("flaky")], strategy(s), policy));
            assert_eq!(
                w.calls("flaky"),
                1,
                "{s}: error doesn't match retry_condition"
            );
            assert_eq!(out["aborted"], true, "{s}");
        }
    }

    #[test]
    fn fallback_replaces_a_failed_step() {
        for s in STRATEGIES {
            let w = world();
            let policy = json!({"type": "fallback", "fallback_count": 2, "fallback_tools": [tool("bad"), tool("ok2")]});
            let out = w.run(chain(vec![tool("bad"), tool("ok")], strategy(s), policy));
            assert_eq!(oks(&out), [true, true], "{s}: {out}");
            assert_eq!(out["results"][0]["fallback"], "ok2", "{s}");
            assert_eq!(out["results"][0]["value"], "ok2:[]", "{s}");

            let w = world();
            let policy = json!({"type": "fallback", "fallback_tools": [tool("bad")]});
            let out = w.run(chain(vec![tool("bad")], strategy(s), policy));
            assert_eq!(out["aborted"], true, "{s}: all fallbacks failed");
        }
    }

    #[test]
    fn retry_strategy_attempts_each_step() {
        let w = world();
        let s = json!({"type": "retry", "max_attempts": 3, "backoff": "Exponential"});
        let out = w.run(chain(vec![tool("flaky")], s, json!({"type": "fail_fast"})));
        assert_eq!(oks(&out), [true]);
        assert_eq!(out["results"][0]["attempts"], 2);
    }

    #[test]
    fn ungranted_tool_fails_its_step_and_never_runs() {
        for s in STRATEGIES {
            for policy in ["fail_fast", "continue_on_error", "retry"] {
                let w = world();
                let mut sneaky = tool("fs.write");
                sneaky["parameters"] = json!({"args": ["/etc/passwd", "x"]});
                let out = w.run(chain(
                    vec![sneaky, tool("ok")],
                    strategy(s),
                    json!({"type": policy, "max_retries": 3}),
                ));
                let r = &out["results"][0];
                assert_eq!(r["ok"], false, "{s}/{policy}");
                assert_eq!(r["attempts"], 1, "{s}/{policy}: a refusal is not retried");
                assert!(
                    r["error"].as_str().unwrap().contains("not granted"),
                    "{s}/{policy}: {r}"
                );
            }
        }
    }

    #[test]
    fn required_capability_must_also_be_granted() {
        let w = world();
        let mut t = tool("ok");
        t["required_capability"] = json!("net.http");
        let out = w.run(chain(
            vec![t],
            strategy("sequential"),
            json!({"type": "fail_fast"}),
        ));
        assert_eq!(oks(&out), [false]);
        assert_eq!(w.calls("ok"), 0, "the tool itself never ran");

        let mut t = tool("ok");
        t["required_capability"] = json!("ok2");
        let out = w.run(chain(
            vec![t],
            strategy("sequential"),
            json!({"type": "fail_fast"}),
        ));
        assert_eq!(oks(&out), [true]);
    }

    #[test]
    fn bindings_feed_parameters_and_conditions() {
        let w = world();
        let mut first = tool("ok");
        first["result_binding"] = json!("a");
        let mut second = tool("ok2");
        second["parameters"] = json!({"args": ["$a", 7]});
        second["condition"] = json!("a");
        let mut third = tool("ok");
        third["condition"] = json!("!a");
        let out = w.run(chain(
            vec![first, second, third],
            strategy("sequential"),
            json!({"type": "fail_fast"}),
        ));
        assert_eq!(out["results"][1]["value"], "ok2:[\"ok:[]\",7]");
        assert_eq!(out["results"][2]["skipped"], true);
        assert_eq!(*w.log.lock().unwrap(), ["ok", "ok2"]);
    }

    #[test]
    fn conditional_strategy_gates_by_position() {
        let w = world();
        let mut first = tool("ok");
        first["result_binding"] = json!("done");
        let s = json!({"type": "conditional", "conditions": ["", "missing", "done"]});
        let out = w.run(chain(
            vec![first, tool("ok2"), tool("ok2")],
            s,
            json!({"type": "fail_fast"}),
        ));
        let skipped: Vec<bool> = out["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["skipped"].as_bool().unwrap())
            .collect();
        assert_eq!(skipped, [false, true, false]);
        assert_eq!(w.calls("ok2"), 1);
    }

    #[test]
    fn map_parameters_are_one_argument() {
        let w = world();
        let mut t = tool("ok");
        t["parameters"] = json!({"path": "a.txt"});
        let out = w.run(chain(
            vec![t],
            strategy("sequential"),
            json!({"type": "fail_fast"}),
        ));
        assert_eq!(out["results"][0]["value"], "ok:[{\"path\":\"a.txt\"}]");
    }

    #[test]
    fn malformed_payloads_are_errors() {
        let cap = ToolchainCap::new(Arc::new(HostCaps::new()));
        for bad in [
            json!({"tools": 5}),
            json!({"tools": [{"parameters": {}}]}),
            json!({"tools": [], "strategy": {"type": "sideways"}}),
            json!({"tools": [], "error_handling": {"type": "shrug"}}),
        ] {
            let arg: Value = serde_json::from_value(bad.clone()).unwrap();
            assert!(cap.call(vec![arg]).is_err(), "{bad}");
        }
        assert!(cap.call(vec![]).is_err());
    }
}
