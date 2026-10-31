//! The events a program pulls with `ui.next_event()`.

use crush_vm::vm::{MapStore, Value};

/// What happened, as a program sees it.
///
/// Every event has the same four fields so a program can test `ev.kind` /
/// `ev.id` without checking a key exists first; the ones that do not apply
/// are the empty string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiEvent {
    /// `"click"`, `"input"`, `"submit"`, `"key"` or `"close"`.
    pub kind: String,
    /// The `id` of the element the event happened on, or `""`.
    pub id: String,
    /// The text of an `input` / `submit` event, or `""`.
    pub value: String,
    /// The key of a `key` event (`"Enter"`, `"Escape"`, `"a"`, …), or `""`.
    pub key: String,
}

impl UiEvent {
    fn new(kind: &str, id: &str, value: &str, key: &str) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            value: value.into(),
            key: key.into(),
        }
    }

    pub fn click(id: &str) -> Self {
        Self::new("click", id, "", "")
    }

    pub fn input(id: &str, value: &str) -> Self {
        Self::new("input", id, value, "")
    }

    pub fn submit(id: &str, value: &str) -> Self {
        Self::new("submit", id, value, "")
    }

    pub fn key(key: &str) -> Self {
        Self::new("key", "", "", key)
    }

    pub fn close() -> Self {
        Self::new("close", "", "", "")
    }

    pub fn is_close(&self) -> bool {
        self.kind == "close"
    }

    /// The map the program receives.
    pub fn to_value(&self) -> Value {
        let mut map = MapStore::new();
        map.insert("kind".into(), Value::Str(self.kind.clone()));
        map.insert("id".into(), Value::Str(self.id.clone()));
        map.insert("value".into(), Value::Str(self.value.clone()));
        map.insert("key".into(), Value::Str(self.key.clone()));
        Value::Map(std::rc::Rc::new(std::cell::RefCell::new(map)))
    }
}
