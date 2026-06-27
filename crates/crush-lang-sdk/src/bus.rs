//! In-memory message-bus host capability for the CRUSH runtime.
//!
//! Provides pub/sub primitives that a CRUSH program can use to communicate
//! with itself or with a host-side broker. This is a synchronous, in-memory
//! implementation suitable for single-process embeddings. A future host can
//! replace it with a real broker (e.g. mycelium gossipsub) by registering a
//! custom `HostCap` under the same capability names.

use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex};

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// Register the in-memory message-bus capabilities on a [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps) {
    let bus = Box::new(InMemoryMessageBus::new());
    caps.register(Box::new(MessageBusPublishCap::new(bus.clone_state())));
    caps.register(Box::new(MessageBusSubscribeCap::new(bus.clone_state())));
    caps.register(Box::new(MessageBusRecvCap::new(bus.clone_state())));
}

#[derive(Debug, Clone)]
struct Message {
    topic: String,
    /// Payload serialized to JSON text to avoid Send issues with Rc<RefCell<...>>.
    payload_json: serde_json::Value,
}

#[derive(Clone)]
struct BusState {
    inner: std::sync::Arc<BusInner>,
}

struct BusInner {
    queues: Mutex<HashMap<String, VecDeque<Message>>>,
    cv: Condvar,
}

impl BusState {
    fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(BusInner {
                queues: Mutex::new(HashMap::new()),
                cv: Condvar::new(),
            }),
        }
    }

    fn publish(&self, topic: String, payload: Value) {
        // **Canonical path**: route through `impl serde::Serialize for Value`
        // (defined on `crush_vm::vm::Value`). The previous local
        // `crush_value_to_json` duplicate is now deleted; the `Serialize`
        // impl is the single source of truth for every JSON consumer.
        // `.unwrap()` is safe: the only failure mode of `serde_json::to_value`
        // for our variant list is unreachable (we never emit a `Map`-key
        // that isn't `String`, never recurse into cyclic data).
        let payload_json = serde_json::to_value(&payload).unwrap();
        let mut queues = self.inner.queues.lock().unwrap();
        queues
            .entry(topic.clone())
            .or_default()
            .push_back(Message { topic, payload_json });
        self.inner.cv.notify_one();
    }

    fn subscribe(&self, topic: &str) {
        let mut queues = self.inner.queues.lock().unwrap();
        queues.entry(topic.to_string()).or_default();
    }

    fn recv(&self) -> Result<Message, String> {
        let mut queues = self.inner.queues.lock().unwrap();
        loop {
            for (_topic, q) in queues.iter_mut() {
                if let Some(msg) = q.pop_front() {
                    return Ok(msg);
                }
            }
            queues = self.inner.cv.wait(queues).map_err(|e| e.to_string())?;
        }
    }
}

struct InMemoryMessageBus {
    state: BusState,
}

impl InMemoryMessageBus {
    fn new() -> Self {
        Self {
            state: BusState::new(),
        }
    }

    fn clone_state(&self) -> BusState {
        self.state.clone()
    }
}

struct MessageBusPublishCap {
    state: BusState,
}

impl MessageBusPublishCap {
    fn new(state: BusState) -> Self {
        Self { state }
    }
}

impl HostCap for MessageBusPublishCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "message_bus.publish".to_string(),
            argc: Some(2),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let topic = crate::caps::value_as_text(&args[0]);
        self.state.publish(topic, args[1].clone());
        Ok(None)
    }
}

struct MessageBusSubscribeCap {
    state: BusState,
}

impl MessageBusSubscribeCap {
    fn new(state: BusState) -> Self {
        Self { state }
    }
}

impl HostCap for MessageBusSubscribeCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "message_bus.subscribe".to_string(),
            argc: Some(1),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let topic = crate::caps::value_as_text(&args[0]);
        self.state.subscribe(&topic);
        Ok(None)
    }
}

struct MessageBusRecvCap {
    state: BusState,
}

impl MessageBusRecvCap {
    fn new(state: BusState) -> Self {
        Self { state }
    }
}

impl HostCap for MessageBusRecvCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "message_bus.recv".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let msg = self.state.recv()?;
        // **Canonical path**: hydrate the stored `payload_json` through
        // `impl<'de> serde::Deserialize<'de> for Value` (in
        // `crush_vm::vm::Value`), then wrap the typed payload in a
        // canonical `Value::Map` envelope and re-emit it via
        // `impl serde::Serialize for Value` — both sides of the
        // round-trip go through the canonical trait impls, matching the
        // JSON.parse cleanup symmetry (input JSON form → canonical
        // Deserialize, output JSON form → canonical Serialize).
        // The legacy `serde_json::Value::Object + to_string +
        // Value::Str(json)` construction is replaced: the receive-side
        // AST manipulation is gone, replaced by typed `Value`
        // composition between two canonical `serde` round-trips.
        let payload: Value = serde_json::from_value(msg.payload_json)
            .map_err(|e| format!("message_bus.recv payload deserialize: {e}"))?;
        let mut map = std::collections::HashMap::new();
        map.insert("topic".to_string(), Value::Str(msg.topic));
        map.insert("payload".to_string(), payload);
        let json = serde_json::to_string(&Value::new_map(map))
            .map_err(|e| format!("message_bus.recv serialize: {e}"))?;
        Ok(Some(Value::Str(json)))
    }
}

// (The previous local `crush_value_to_json` duplicate of the canonical
// `impl serde::Serialize for Value` in `crush-vm/src/vm.rs` has been
// deleted. The `publish` callsite now invokes `serde_json::to_value` on
// the `Value` directly, eliminating the drift between util's colon-form
// `handle:N` (deleted) and the rest of the bus's angle-bracket `<handle N>`
// (now the only canonical form, matching `Display` for line rendering).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_then_recv_roundtrip() {
        let bus = InMemoryMessageBus::new();
        let pub_cap = MessageBusPublishCap::new(bus.clone_state());
        let sub_cap = MessageBusSubscribeCap::new(bus.clone_state());
        let recv_cap = MessageBusRecvCap::new(bus.clone_state());

        sub_cap.call(vec![Value::Str("t1".to_string())]).unwrap();
        pub_cap
            .call(vec![
                Value::Str("t1".to_string()),
                Value::Str("hello".to_string()),
            ])
            .unwrap();

        let result = recv_cap.call(vec![]).unwrap();
        // Substring asserts on the JSON-envelope output produced by the
        // canonical `impl Serialize for Value` (called from
        // MessageBusRecvCap after the receive-path Deserialize). The
        // retrieve-side flow is now: stored payload_json → canonical
        // `Deserialize` → typed `Value` → canonical `Serialize` →
        // JSON string envelope. The substring checks confirm the typed
        // payload re-emitted intact through the canonical round-trip.
        let s = crate::caps::value_as_text(&result.unwrap());
        assert!(s.contains("\"topic\":\"t1\""));
        assert!(s.contains("\"payload\":\"hello\""));
    }
}
