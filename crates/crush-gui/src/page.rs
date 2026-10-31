//! Wiring from DOM events to [`UiEvent`]s. Shared by the desktop and the
//! headless backends, so the headless tests exercise the same handlers a
//! real window does.
//!
//! A page is whatever markup the program last rendered, inside one content
//! element. After every render [`Wiring::rewire`] walks that element and
//! registers, on arniko's [`EventRouter`]:
//!
//! * a click handler on **every** node below an element that has an `id`
//!   (a click lands on the innermost node, often a text node inside the
//!   `<button>`, and arniko's sink matches the exact target node only), which
//!   reports `click` with the nearest enclosing id;
//! * `input` and `Enter` (-> `submit`) handlers on each `<input id=…>`.
//!
//! Node ids are reused by the DOM's arena, so each registration is stamped
//! with a generation and goes quiet once a newer render replaced it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arniko::reactive::EventRouter;
use arniko::reactive::keyboard_types::Key;
use bliss::dom::{BaseDocument, local_name};
use parking_lot::Mutex;

use crate::backend::EventTx;
use crate::event::UiEvent;

/// Owns what must outlive one render: the generation counter, the event
/// channel, and the nodes registered last time.
pub struct Wiring {
    generation: Arc<AtomicU64>,
    tx: EventTx,
    registered: Mutex<Vec<usize>>,
    /// Last text seen per input id, so `Enter` can report it.
    values: Arc<Mutex<HashMap<String, String>>>,
}

impl Wiring {
    pub fn new(tx: EventTx) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            tx,
            registered: Mutex::new(Vec::new()),
            values: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Report non-printing keys (Escape, Tab, arrows, …) as `key` events.
    /// `Enter` is not one: inside an input it is `submit`.
    pub fn install_global_keys(&self, router: &EventRouter) {
        let tx = self.tx.clone();
        router.on_keydown(move |ev| {
            if matches!(ev.key, Key::Character(_) | Key::Enter) {
                return;
            }
            tx.send(UiEvent::key(&ev.key.to_string()));
        });
    }

    /// Replace the handlers with ones for the page now under `root`.
    pub fn rewire(&self, router: &EventRouter, doc: &BaseDocument, root: usize) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut old = self.registered.lock();
            for node in old.drain(..) {
                router.off_click(node);
                router.off_input(node);
            }
        }
        self.values.lock().clear();

        let mut registered = Vec::new();
        let mut stack = vec![(root, None::<String>)];
        while let Some((node_id, inherited)) = stack.pop() {
            let Some(node) = doc.get_node(node_id) else {
                continue;
            };
            let own_id = node
                .element_data()
                .and_then(|e| e.id.as_ref())
                .map(|a| a.to_string())
                .filter(|s| !s.is_empty());
            let owner = own_id.clone().or(inherited);

            if let (true, Some(id)) = (node_id != root, owner.clone()) {
                let tx = self.tx.clone();
                let live = self.generation.clone();
                router.on_click(node_id, move || {
                    if live.load(Ordering::SeqCst) == generation {
                        tx.send(UiEvent::click(&id));
                    }
                });
                registered.push(node_id);
            }

            if let (Some(id), true) = (
                own_id,
                node.data.is_element_with_tag_name(&local_name!("input")),
            ) {
                let (tx, live, values) = (
                    self.tx.clone(),
                    self.generation.clone(),
                    self.values.clone(),
                );
                self.values.lock().insert(
                    id.clone(),
                    node.attr(local_name!("value")).unwrap_or("").to_string(),
                );
                let input_id = id.clone();
                router.on_input(node_id, move |value| {
                    if live.load(Ordering::SeqCst) == generation {
                        values.lock().insert(input_id.clone(), value.clone());
                        tx.send(UiEvent::input(&input_id, &value));
                    }
                });
                let (tx, live, values) = (
                    self.tx.clone(),
                    self.generation.clone(),
                    self.values.clone(),
                );
                router.on_keydown_node(node_id, move |ev| {
                    if ev.key == Key::Enter && live.load(Ordering::SeqCst) == generation {
                        let value = values.lock().get(&id).cloned().unwrap_or_default();
                        tx.send(UiEvent::submit(&id, &value));
                    }
                });
                registered.push(node_id);
            }

            for child in node.children.iter().rev() {
                stack.push((*child, owner.clone()));
            }
        }
        *self.registered.lock() = registered;
    }
}
