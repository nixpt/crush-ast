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
        let payload_json = crush_value_to_json(&payload);
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
        let mut map = serde_json::Map::new();
        map.insert("topic".to_string(), serde_json::Value::String(msg.topic));
        map.insert("payload".to_string(), msg.payload_json);
        Ok(Some(Value::Str(
            serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default(),
        )))
    }
}

fn crush_value_to_json(v: &crush_vm::vm::Value) -> serde_json::Value {
    match v {
        crush_vm::vm::Value::Null => serde_json::Value::Null,
        crush_vm::vm::Value::Bool(b) => serde_json::Value::Bool(*b),
        crush_vm::vm::Value::Int(i) => serde_json::Value::Number((*i).into()),
        crush_vm::vm::Value::Float(f) => {
            serde_json::Value::Number(serde_json::Number::from_f64(*f).unwrap_or(0.into()))
        }
        crush_vm::vm::Value::Str(s) => serde_json::Value::String(s.clone()),
        crush_vm::vm::Value::Array(a) => {
            serde_json::Value::Array(a.borrow().iter().map(crush_value_to_json).collect())
        }
        crush_vm::vm::Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), crush_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        crush_vm::vm::Value::Error(e) => serde_json::Value::String(format!("error({})", e)),
        crush_vm::vm::Value::Bytes(b) => serde_json::Value::String(format!("<{} bytes>", b.len())),
        crush_vm::vm::Value::Handle(id) => serde_json::Value::String(format!("<handle {}>", id)),
    }
}

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
        let s = crate::caps::value_as_text(&result.unwrap());
        assert!(s.contains("\"topic\":\"t1\""));
        assert!(s.contains("\"payload\":\"hello\""));
    }
}
