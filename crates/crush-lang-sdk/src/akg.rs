//! In-memory knowledge-graph host capability for the CRUSH runtime.
//!
//! Provides a minimal AKG (Agent Knowledge Graph) interface: write/read/search
//! knowledge units by ID. This is a synchronous, in-memory store suitable for
//! single-process embeddings. A production host can replace these caps with a
//! persistent graph database by registering custom `HostCap` handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// Register the in-memory AKG capabilities on a [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps) {
    let state = Arc::new(Mutex::new(AkgState::new()));
    caps.register(Box::new(AkgWriteCap::new(Arc::clone(&state))));
    caps.register(Box::new(AkgReadCap::new(Arc::clone(&state))));
    caps.register(Box::new(AkgSearchCap::new(state)));
}

struct AkgState {
    units: HashMap<String, serde_json::Value>,
}

impl AkgState {
    fn new() -> Self {
        Self {
            units: HashMap::new(),
        }
    }

    fn write(&mut self, id: String, unit: serde_json::Value) {
        self.units.insert(id, unit);
    }

    fn read(&self, id: &str) -> Option<serde_json::Value> {
        self.units.get(id).cloned()
    }

    fn search(&self, query: &str) -> Vec<serde_json::Value> {
        let q = query.to_lowercase();
        self.units
            .iter()
            .filter(|(_id, unit)| unit.to_string().to_lowercase().contains(&q))
            .map(|(id, unit)| {
                serde_json::json!({
                    "id": id,
                    "unit": unit,
                })
            })
            .collect()
    }
}

struct AkgWriteCap {
    state: Arc<Mutex<AkgState>>,
}

impl AkgWriteCap {
    fn new(state: Arc<Mutex<AkgState>>) -> Self {
        Self { state }
    }
}

impl HostCap for AkgWriteCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "akg.write".to_string(),
            argc: Some(2),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = crate::caps::value_as_text(&args[0]);
        let unit_json = crate::caps::value_as_text(&args[1]);
        let unit =
            serde_json::from_str(&unit_json).map_err(|e| format!("akg.write invalid JSON: {e}"))?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        state.write(id, unit);
        Ok(None)
    }
}

struct AkgReadCap {
    state: Arc<Mutex<AkgState>>,
}

impl AkgReadCap {
    fn new(state: Arc<Mutex<AkgState>>) -> Self {
        Self { state }
    }
}

impl HostCap for AkgReadCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "akg.read".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let id = crate::caps::value_as_text(&args[0]);
        let state = self.state.lock().map_err(|e| e.to_string())?;
        match state.read(&id) {
            Some(unit) => Ok(Some(Value::Str(
                serde_json::to_string(&unit).unwrap_or_default(),
            ))),
            None => Ok(Some(Value::Null)),
        }
    }
}

struct AkgSearchCap {
    state: Arc<Mutex<AkgState>>,
}

impl AkgSearchCap {
    fn new(state: Arc<Mutex<AkgState>>) -> Self {
        Self { state }
    }
}

impl HostCap for AkgSearchCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "akg.search".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let query = crate::caps::value_as_text(&args[0]);
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let results = state.search(&query);
        Ok(Some(Value::Array(
            results
                .into_iter()
                .map(|v| Value::Str(serde_json::to_string(&v).unwrap_or_default()))
                .collect(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_search_roundtrip() {
        let state = Arc::new(Mutex::new(AkgState::new()));
        let write = AkgWriteCap::new(Arc::clone(&state));
        let read = AkgReadCap::new(Arc::clone(&state));
        let search = AkgSearchCap::new(Arc::clone(&state));

        write
            .call(vec![
                Value::Str("u1".to_string()),
                Value::Str(r#"{"title":"hello","tags":["greeting"]}"#.to_string()),
            ])
            .unwrap();

        let found = read.call(vec![Value::Str("u1".to_string())]).unwrap();
        assert!(matches!(found, Some(Value::Str(_))));

        let results = search
            .call(vec![Value::Str("greeting".to_string())])
            .unwrap();
        if let Value::Array(arr) = results.unwrap() {
            assert_eq!(arr.len(), 1);
        } else {
            panic!("expected array");
        }
    }
}
