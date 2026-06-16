//! Task/subprocess host capability for the CRUSH runtime.
//!
//! Allows a CRUSH program to spawn, stop, and list lightweight host
//! subprocesses. This is intentionally simple: each task is a `std::process::Command`
//! execution with a captured stdout. A future host can replace these caps with
//! a richer scheduler by registering custom `HostCap` handlers.

use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crush_vm::{HostCap, HostCapSpec, HostCaps};
use crush_vm::vm::Value;

/// Register the task-management capabilities on a [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps) {
    let state = Arc::new(Mutex::new(TaskState::new()));
    caps.register(Box::new(TaskStartCap::new(Arc::clone(&state))));
    caps.register(Box::new(TaskStopCap::new(Arc::clone(&state))));
    caps.register(Box::new(TaskListCap::new(state)));
}

struct TaskState {
    next_id: u64,
    tasks: HashMap<String, Child>,
}

impl TaskState {
    fn new() -> Self {
        Self {
            next_id: 1,
            tasks: HashMap::new(),
        }
    }

    fn start(&mut self, name: &str, command: &str, args: &[String]) -> Result<String, String> {
        let id = format!("{}-{}", name, self.next_id);
        self.next_id += 1;

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("task start {command}: {e}"))?;
        self.tasks.insert(id.clone(), child);
        Ok(id)
    }

    fn stop(&mut self, task_id: &str) -> Result<(), String> {
        let mut child = self
            .tasks
            .remove(task_id)
            .ok_or_else(|| format!("task not found: {task_id}"))?;
        child.kill().map_err(|e| format!("task stop {task_id}: {e}"))?;
        child
            .wait()
            .map_err(|e| format!("task wait {task_id}: {e}"))?;
        Ok(())
    }

    fn list(&self) -> Vec<serde_json::Value> {
        self.tasks
            .keys()
            .map(|id| {
                serde_json::json!({
                    "task_id": id,
                    "running": true,
                })
            })
            .collect()
    }
}

struct TaskStartCap {
    state: Arc<Mutex<TaskState>>,
}

impl TaskStartCap {
    fn new(state: Arc<Mutex<TaskState>>) -> Self {
        Self { state }
    }
}

impl HostCap for TaskStartCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "task.start".to_string(),
            argc: None,
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        if args.len() < 2 {
            return Err("task.start requires name and command".into());
        }
        let name = crate::caps::value_as_text(&args[0]);
        let command = crate::caps::value_as_text(&args[1]);
        let task_args: Vec<String> = args[2..]
            .iter()
            .map(|v| crate::caps::value_as_text(v))
            .collect();

        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let id = state.start(&name, &command, &task_args)?;
        Ok(Some(Value::Str(id)))
    }
}

struct TaskStopCap {
    state: Arc<Mutex<TaskState>>,
}

impl TaskStopCap {
    fn new(state: Arc<Mutex<TaskState>>) -> Self {
        Self { state }
    }
}

impl HostCap for TaskStopCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "task.stop".to_string(),
            argc: Some(1),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let task_id = crate::caps::value_as_text(&args[0]);
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        state.stop(&task_id)?;
        Ok(None)
    }
}

struct TaskListCap {
    state: Arc<Mutex<TaskState>>,
}

impl TaskListCap {
    fn new(state: Arc<Mutex<TaskState>>) -> Self {
        Self { state }
    }
}

impl HostCap for TaskListCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "task.list".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let rows = state.list();
        Ok(Some(Value::Array(
            rows.into_iter()
                .map(|v| {
                    Value::Str(serde_json::to_string(&v).unwrap_or_default())
                })
                .collect(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_stop_sleep_task() {
        let state = Arc::new(Mutex::new(TaskState::new()));
        let start = TaskStartCap::new(Arc::clone(&state));
        let stop = TaskStopCap::new(Arc::clone(&state));
        let list = TaskListCap::new(Arc::clone(&state));

        let id = start
            .call(vec![
                Value::Str("sleep".to_string()),
                Value::Str("sleep".to_string()),
                Value::Str("10".to_string()),
            ])
            .unwrap()
            .unwrap();

        let ids = list.call(vec![]).unwrap().unwrap();
        assert!(matches!(ids, Value::Array(_)));

        stop.call(vec![id]).unwrap();
    }
}
