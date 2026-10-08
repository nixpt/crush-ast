//! CRUSH-158: host-supplied backends for `ai_native.query` and
//! `ai_native.agent_delegation`.
//!
//! crush-ast ships no model or agent backend. A host implements
//! [`QueryProvider`] and/or [`DelegationBackend`] and hands it to
//! [`HostCapsBuilder`](crate::HostCapsBuilder); with `ai_native(true)` the
//! matching gate then runs the backend instead of the echo stub. Without the
//! grant, nothing is registered whatever backend was supplied.
//!
//! Delegation keeps the backend small — report an agent's standing, run a
//! task on one agent — and does the rest here as pure code: picking agents
//! (`first_available`, `broadcast`, `best`, `round_robin`) and validating each
//! result against `expected_format` (`json`, `structured`, `text`). No
//! polling, no filesystem, no fixed paths: everything the selection knows
//! comes from the backend.

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec};
use serde_json::{Map, Value as Json, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// What a program asked `ai_native.query` for (the compiled payload).
#[derive(Debug, Clone, PartialEq)]
pub struct QueryRequest {
    pub query: String,
    pub result_type: Option<String>,
    pub context: Map<String, Json>,
}

/// Answers `ai_native.query`. Implemented by the host (a model client, a
/// cache, a test fake).
pub trait QueryProvider: Send + Sync {
    /// The answer, as JSON (a bare string for plain text). An `Err` becomes
    /// the cap's error.
    fn query(&self, request: &QueryRequest) -> Result<Json, String>;
}

/// An agent's standing, as the backend sees it right now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgentStatus {
    /// Can take a task now.
    pub available: bool,
    /// Higher is better; used by the `best` strategy.
    pub rating: f64,
}

/// Runs delegated tasks. Implemented by the host (a fleet dispatcher, a
/// local worker pool, a test fake).
pub trait DelegationBackend: Send + Sync {
    /// Standing of `agent`, or `None` if the backend doesn't know it.
    fn status(&self, agent: &str) -> Option<AgentStatus>;
    /// Run `task` on `agent` and return its result text.
    fn dispatch(&self, agent: &str, task: &str) -> Result<String, String>;
}

/// `ai_native.query` backed by a [`QueryProvider`]. Returns
/// `{ok: true, kind: "query", result: <answer>}`.
pub struct QueryCap {
    provider: Arc<dyn QueryProvider>,
}

impl QueryCap {
    pub fn new(provider: Arc<dyn QueryProvider>) -> Self {
        Self { provider }
    }
}

impl HostCap for QueryCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "ai_native.query".into(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let p = payload(&args, "ai_native.query")?;
        let request = QueryRequest {
            query: p
                .get("query")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            result_type: p
                .get("result_type")
                .and_then(Json::as_str)
                .map(String::from),
            context: p
                .get("context")
                .and_then(Json::as_object)
                .cloned()
                .unwrap_or_default(),
        };
        let result = self
            .provider
            .query(&request)
            .map_err(|e| format!("ai_native.query: {e}"))?;
        to_value(json!({"ok": true, "kind": "query", "result": result}))
    }
}

/// `ai_native.agent_delegation` backed by a [`DelegationBackend`].
///
/// Returns `{ok, kind, task, strategy, agents, results}`: `agents` is the
/// selection, `results` one `{agent, status, result | error}` per selected
/// agent (`status` is `done`, `validation_failed` or `dispatch_failed`), and
/// `ok` is true iff at least one agent was selected and every one is `done`.
pub struct DelegationCap {
    backend: Arc<dyn DelegationBackend>,
    round_robin: AtomicU64,
}

impl DelegationCap {
    pub fn new(backend: Arc<dyn DelegationBackend>) -> Self {
        Self {
            backend,
            round_robin: AtomicU64::new(0),
        }
    }
}

impl HostCap for DelegationCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "ai_native.agent_delegation".into(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let p = payload(&args, "ai_native.agent_delegation")?;
        let task = p.get("task").and_then(Json::as_str).unwrap_or_default();
        let agents: Vec<String> = p
            .get("agents")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Json::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let strategy = p
            .get("strategy")
            .and_then(|s| s.get("type"))
            .and_then(Json::as_str)
            .unwrap_or("first_available");
        let format = p.get("expected_format").and_then(Json::as_str);
        let wrap = |e: String| format!("ai_native.agent_delegation: {e}");
        let format = format.map(Format::parse).transpose().map_err(wrap)?;

        let selected =
            select(strategy, &agents, self.backend.as_ref(), &self.round_robin).map_err(wrap)?;
        let results: Vec<Json> = selected
            .iter()
            .map(|agent| match self.backend.dispatch(agent, task) {
                Err(e) => json!({"agent": agent, "status": "dispatch_failed", "error": e}),
                Ok(r) => match format.map_or(Ok(()), |f| f.validate(&r)) {
                    Ok(()) => json!({"agent": agent, "status": "done", "result": r}),
                    Err(e) => json!({"agent": agent, "status": "validation_failed", "error": e}),
                },
            })
            .collect();
        let ok = !results.is_empty() && results.iter().all(|r| r["status"] == "done");
        to_value(json!({
            "ok": ok, "kind": "agent_delegation", "task": task, "strategy": strategy,
            "agents": selected, "results": results,
        }))
    }
}

/// Pick the agents a task goes to. Errors on an unknown strategy and on an
/// empty agent list for the single-agent strategies; `first_available` with
/// nobody available selects nobody (the caller reports `ok: false`).
pub fn select(
    strategy: &str,
    agents: &[String],
    backend: &dyn DelegationBackend,
    round_robin: &AtomicU64,
) -> Result<Vec<String>, String> {
    let first_available = || {
        agents
            .iter()
            .find(|a| backend.status(a).is_some_and(|s| s.available))
            .cloned()
            .into_iter()
            .collect()
    };
    match strategy {
        "first_available" => Ok(first_available()),
        "broadcast" => Ok(agents.to_vec()),
        "best" => {
            // Highest-rated available agent; ties keep list order.
            let mut best: Option<(&String, f64)> = None;
            for a in agents {
                if let Some(s) = backend.status(a).filter(|s| s.available)
                    && best.is_none_or(|(_, r)| s.rating > r)
                {
                    best = Some((a, s.rating));
                }
            }
            Ok(best.map(|(a, _)| a.clone()).into_iter().collect())
        }
        "round_robin" => {
            if agents.is_empty() {
                return Err("round_robin needs at least one agent".into());
            }
            let n = round_robin.fetch_add(1, Ordering::Relaxed);
            Ok(vec![agents[(n % agents.len() as u64) as usize].clone()])
        }
        other => Err(format!("delegation strategy `{other}` is not supported")),
    }
}

/// `expected_format` of a delegation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    /// Any valid JSON.
    Json,
    /// A JSON object.
    Structured,
    /// Anything.
    Text,
}

impl Format {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "json" => Ok(Format::Json),
            "structured" => Ok(Format::Structured),
            "text" => Ok(Format::Text),
            other => Err(format!("unknown expected_format `{other}`")),
        }
    }

    pub fn validate(self, result: &str) -> Result<(), String> {
        match self {
            Format::Text => Ok(()),
            Format::Json => serde_json::from_str::<Json>(result)
                .map(drop)
                .map_err(|e| format!("expected JSON: {e}")),
            Format::Structured => match serde_json::from_str::<Json>(result) {
                Ok(Json::Object(_)) => Ok(()),
                Ok(other) => Err(format!("expected a JSON object, got {other}")),
                Err(e) => Err(format!("expected a JSON object: {e}")),
            },
        }
    }
}

fn payload(args: &[Value], cap: &str) -> Result<Json, String> {
    let p = args
        .first()
        .ok_or_else(|| format!("{cap}: missing payload"))?;
    serde_json::to_value(p).map_err(|e| format!("{cap}: {e}"))
}

fn to_value(j: Json) -> Result<Option<Value>, String> {
    serde_json::from_value(j)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct Agents {
        status: HashMap<&'static str, AgentStatus>,
        replies: HashMap<&'static str, Result<&'static str, &'static str>>,
        dispatched: Mutex<Vec<String>>,
    }

    impl DelegationBackend for Agents {
        fn status(&self, agent: &str) -> Option<AgentStatus> {
            self.status.get(agent).copied()
        }
        fn dispatch(&self, agent: &str, task: &str) -> Result<String, String> {
            self.dispatched
                .lock()
                .unwrap()
                .push(format!("{agent}:{task}"));
            match self.replies.get(agent) {
                Some(Ok(r)) => Ok(r.to_string()),
                Some(Err(e)) => Err(e.to_string()),
                None => Ok(String::new()),
            }
        }
    }

    fn st(available: bool, rating: f64) -> AgentStatus {
        AgentStatus { available, rating }
    }

    fn agents() -> Agents {
        Agents {
            status: HashMap::from([
                ("a", st(false, 0.9)),
                ("b", st(true, 0.4)),
                ("c", st(true, 0.7)),
            ]),
            replies: HashMap::from([
                ("a", Ok("{\"x\":1}")),
                ("b", Ok("[1,2]")),
                ("c", Err("crashed")),
            ]),
            dispatched: Mutex::new(Vec::new()),
        }
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn selection_strategies() {
        let be = agents();
        let rr = AtomicU64::new(0);
        let all = names(&["a", "b", "c"]);
        assert_eq!(select("first_available", &all, &be, &rr).unwrap(), ["b"]);
        assert_eq!(
            select("broadcast", &all, &be, &rr).unwrap(),
            ["a", "b", "c"]
        );
        assert_eq!(
            select("best", &all, &be, &rr).unwrap(),
            ["c"],
            "a rates higher but is busy"
        );
        let rr_picks: Vec<_> = (0..4)
            .map(|_| select("round_robin", &all, &be, &rr).unwrap().remove(0))
            .collect();
        assert_eq!(rr_picks, ["a", "b", "c", "a"]);
    }

    #[test]
    fn selection_edge_cases() {
        let be = agents();
        let rr = AtomicU64::new(0);
        assert!(
            select("first_available", &names(&["a", "zz"]), &be, &rr)
                .unwrap()
                .is_empty()
        );
        assert!(
            select("best", &names(&["zz"]), &be, &rr)
                .unwrap()
                .is_empty()
        );
        assert!(select("round_robin", &[], &be, &rr).is_err());
        assert!(select("consensus", &names(&["a"]), &be, &rr).is_err());
        // Ties keep list order.
        let tie = Agents {
            status: HashMap::from([("p", st(true, 0.5)), ("q", st(true, 0.5))]),
            replies: HashMap::new(),
            dispatched: Mutex::new(Vec::new()),
        };
        assert_eq!(
            select("best", &names(&["p", "q"]), &tie, &rr).unwrap(),
            ["p"]
        );
    }

    #[test]
    fn format_validation() {
        assert!(Format::Json.validate("[1]").is_ok());
        assert!(Format::Json.validate("nope").is_err());
        assert!(Format::Structured.validate("{\"a\":1}").is_ok());
        assert!(Format::Structured.validate("[1]").is_err());
        assert!(Format::Text.validate("anything").is_ok());
        assert!(Format::parse("yaml").is_err());
    }

    fn run(cap: &DelegationCap, p: Json) -> Result<Json, String> {
        let v: Value = serde_json::from_value(p).unwrap();
        cap.call(vec![v])
            .map(|v| serde_json::to_value(v.unwrap()).unwrap())
    }

    #[test]
    fn delegation_dispatches_and_validates() {
        let be = Arc::new(agents());
        let cap = DelegationCap::new(be.clone());
        let out = run(
            &cap,
            json!({"task": "t1", "agents": ["a", "b", "c"], "strategy": {"type": "broadcast"},
                   "expected_format": "structured"}),
        )
        .unwrap();
        let status: Vec<&str> = out["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["status"].as_str().unwrap())
            .collect();
        assert_eq!(status, ["done", "validation_failed", "dispatch_failed"]);
        assert_eq!(out["ok"], false);
        assert_eq!(*be.dispatched.lock().unwrap(), ["a:t1", "b:t1", "c:t1"]);

        let out = run(
            &cap,
            json!({"task": "t2", "agents": ["a", "b"], "strategy": {"type": "first_available"}}),
        )
        .unwrap();
        assert_eq!(out["agents"], json!(["b"]));
        assert_eq!(out["results"][0]["result"], "[1,2]");
        assert_eq!(out["ok"], true);
    }

    #[test]
    fn delegation_with_nobody_available_dispatches_nothing() {
        let be = Arc::new(agents());
        let cap = DelegationCap::new(be.clone());
        let out = run(
            &cap,
            json!({"task": "t", "agents": ["a"], "strategy": {"type": "first_available"}}),
        )
        .unwrap();
        assert_eq!(out["ok"], false);
        assert_eq!(out["agents"], json!([]));
        assert!(be.dispatched.lock().unwrap().is_empty());
    }

    #[test]
    fn delegation_rejects_unknown_strategy_and_format() {
        let cap = DelegationCap::new(Arc::new(agents()));
        assert!(
            run(
                &cap,
                json!({"task": "t", "agents": ["a"], "strategy": {"type": "hierarchical"}})
            )
            .is_err()
        );
        assert!(
            run(&cap, json!({"task": "t", "agents": ["b"], "strategy": {"type": "broadcast"}, "expected_format": "xml"}))
                .is_err()
        );
    }

    struct Echo;
    impl QueryProvider for Echo {
        fn query(&self, r: &QueryRequest) -> Result<Json, String> {
            if r.query.is_empty() {
                return Err("empty query".into());
            }
            Ok(json!({"answer": r.query.to_uppercase(), "type": r.result_type, "ctx": r.context}))
        }
    }

    #[test]
    fn query_cap_passes_the_request_to_the_provider() {
        let cap = QueryCap::new(Arc::new(Echo));
        let p: Value = serde_json::from_value(
            json!({"query": "hi", "result_type": "string", "context": {"k": "v"}}),
        )
        .unwrap();
        let out = serde_json::to_value(cap.call(vec![p]).unwrap().unwrap()).unwrap();
        assert_eq!(
            out,
            json!({"ok": true, "kind": "query",
                   "result": {"answer": "HI", "type": "string", "ctx": {"k": "v"}}})
        );
        let empty: Value = serde_json::from_value(json!({"query": ""})).unwrap();
        assert!(cap.call(vec![empty]).unwrap_err().contains("empty query"));
    }
}
