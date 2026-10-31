//! The headless backend: a real bliss DOM with no window, for tests.
//!
//! [`HeadlessApp`] runs a program on its own thread, parses every page it
//! renders into a bliss document, and lets a test act on that document by
//! element id. Events go through the same handlers a window uses
//! ([`Wiring`] + arniko's event sink); only winit's delivery of the raw
//! mouse/keyboard event to the DOM is skipped.
//!
//! There is no timing in it: after each action [`HeadlessApp`] waits until
//! the program is blocked in `ui.next_event()` again (or has ended), so the
//! DOM it then reports is the program's reaction to that event.

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use anyhow::{Result, anyhow, bail};
use arniko::reactive::keyboard_types::{Code, Key, Location, Modifiers};
use arniko::reactive::{ArnikoEventSink, EventRouter, event_router};
use bliss::dom::{BaseDocument, DocumentConfig};
use bliss::html::{HtmlDocument, HtmlProvider};
use bliss::traits::events::{
    BlissInputEvent, BlissKeyEvent, BlissPointerEvent, BlissPointerId, DomEvent, DomEventData,
    EventPhase, EventSink, KeyState, MouseEventButton, MouseEventButtons, PointerCoords,
    PointerDetails,
};
use bliss::traits::net::DummyNetProvider;
use parking_lot::Mutex;

use crate::backend::{EventTx, Request, link};
use crate::event::UiEvent;
use crate::page::Wiring;
use crate::run::{Grants, RunEnd, Source, execute};

const CONTENT_ID: &str = "crush-ui-content";

/// How the program ended.
pub type Outcome = std::result::Result<RunEnd, String>;

pub struct HeadlessApp {
    doc: HtmlDocument,
    content: usize,
    router: EventRouter,
    sink: ArnikoEventSink,
    wiring: Wiring,
    requests: Receiver<Request>,
    events: EventTx,
    thread: Option<JoinHandle<Outcome>>,
    outcome: Option<Outcome>,
    output: Arc<Mutex<String>>,
    markup: Option<String>,
    title: String,
    renders: usize,
}

impl HeadlessApp {
    /// Start `source` with `grants` and run it until it first waits for an
    /// event (or ends). The program thread is detached from the test thread:
    /// a refusal or a compile error shows up as [`HeadlessApp::outcome`].
    pub fn start(source: Source, grants: Grants) -> Self {
        let (link, surface) = link();
        let output = Arc::new(Mutex::new(String::new()));
        let sink_out = output.clone();
        let thread = std::thread::spawn(move || -> Outcome {
            let mut write = |part: &str| sink_out.lock().push_str(part);
            execute(&source, &grants, Arc::new(link), &mut write).map_err(|e| format!("{e:#}"))
        });

        let doc = HtmlDocument::from_html(
            &format!(
                r#"<!DOCTYPE html><html><head></head><body><div id="{CONTENT_ID}"></div></body></html>"#
            ),
            DocumentConfig {
                net_provider: Some(Arc::new(DummyNetProvider)),
                html_parser_provider: Some(Arc::new(HtmlProvider)),
                ..Default::default()
            },
        );
        let (router, sink) = event_router();
        let content = doc
            .get_element_by_id(CONTENT_ID)
            .expect("skeleton has the content element");
        let wiring = Wiring::new(surface.events.clone());
        wiring.install_global_keys(&router);

        let mut app = Self {
            doc,
            content,
            router,
            sink,
            wiring,
            requests: surface.requests,
            events: surface.events,
            thread: Some(thread),
            outcome: None,
            output,
            markup: None,
            title: String::new(),
            renders: 0,
        };
        app.settle();
        app
    }

    /// Apply the program's requests until it blocks for an event or ends.
    fn settle(&mut self) {
        loop {
            match self.requests.recv() {
                Ok(Request::Render(markup)) => {
                    {
                        let mut mutator = self.doc.mutate();
                        mutator.set_inner_html(self.content, &markup);
                    }
                    self.wiring.rewire(&self.router, &self.doc, self.content);
                    self.markup = Some(markup);
                    self.renders += 1;
                }
                Ok(Request::Title(title)) => self.title = title,
                Ok(Request::NextEvent) => return,
                Err(_) => {
                    // The program's end of the link is gone: it has finished.
                    if let Some(thread) = self.thread.take() {
                        self.outcome = Some(
                            thread
                                .join()
                                .unwrap_or_else(|_| Err("the program thread panicked".into())),
                        );
                    }
                    return;
                }
            }
        }
    }

    // ---- observing ------------------------------------------------------

    /// The text of the page the program last rendered (`""` before the first).
    pub fn text(&self) -> String {
        self.doc
            .get_node(self.content)
            .map(|n| self.doc.tree()[n.id].text_content())
            .unwrap_or_default()
    }

    /// The markup the program last passed to `ui.render`.
    pub fn markup(&self) -> Option<&str> {
        self.markup.as_deref()
    }

    /// The text of the element with this id, if the page has one.
    pub fn text_of(&self, id: &str) -> Option<String> {
        let node = self.doc.get_element_by_id(id)?;
        Some(self.doc.tree()[node].text_content())
    }

    /// The `value` attribute of an element (an input's rendered text).
    pub fn value_of(&self, id: &str) -> Option<String> {
        let node = self.doc.get_element_by_id(id)?;
        self.doc.tree()[node]
            .attr(bliss::dom::local_name!("value"))
            .map(str::to_string)
    }

    pub fn has_id(&self, id: &str) -> bool {
        self.doc.get_element_by_id(id).is_some()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// How many pages the program has rendered.
    pub fn renders(&self) -> usize {
        self.renders
    }

    /// Everything the program printed with `io.print`.
    pub fn output(&self) -> String {
        self.output.lock().clone()
    }

    /// `Some` once the program has ended.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    pub fn is_running(&self) -> bool {
        self.outcome.is_none()
    }

    // ---- acting ---------------------------------------------------------

    fn node(&self, id: &str) -> Result<usize> {
        self.doc
            .get_element_by_id(id)
            .ok_or_else(|| anyhow!("no element with id {id:?} on the page"))
    }

    fn dispatch(&mut self, node: usize, data: DomEventData) -> Result<()> {
        if !self.is_running() {
            bail!("the program has ended");
        }
        let event = DomEvent::new(node, data);
        let before = self.events.sent();
        self.sink
            .on_dom_event(self.doc.id(), &event, EventPhase::Target, node);
        // An input the page does not report (a printable key, say) leaves the
        // program blocked where it was: there is nothing to wait for.
        if self.events.sent() > before {
            self.settle();
        }
        Ok(())
    }

    /// Click the element with this id — on its innermost child, as a real
    /// click lands on the text inside a button.
    pub fn click(&mut self, id: &str) -> Result<()> {
        let mut node = self.node(id)?;
        while let Some(&child) = self.doc.tree()[node].children.first() {
            node = child;
        }
        let pointer = BlissPointerEvent {
            id: BlissPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: 0.0,
                page_y: 0.0,
                screen_x: 0.0,
                screen_y: 0.0,
                client_x: 0.0,
                client_y: 0.0,
            },
            button: MouseEventButton::Main,
            buttons: MouseEventButtons::default(),
            mods: Modifiers::default(),
            details: PointerDetails::default(),
        };
        self.dispatch(node, DomEventData::Click(pointer))
    }

    /// Replace the text of the input with this id, as typing does.
    pub fn type_into(&mut self, id: &str, text: &str) -> Result<()> {
        let node = self.node(id)?;
        self.dispatch(
            node,
            DomEventData::Input(BlissInputEvent {
                value: text.to_string(),
            }),
        )
    }

    fn key_event(key: Key) -> BlissKeyEvent {
        BlissKeyEvent {
            key,
            code: Code::Unidentified,
            modifiers: Modifiers::default(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        }
    }

    /// Press Enter in the input with this id.
    pub fn press_enter(&mut self, id: &str) -> Result<()> {
        let node = self.node(id)?;
        self.dispatch(node, DomEventData::KeyDown(Self::key_event(Key::Enter)))
    }

    /// Press a non-printing key (`Escape`, `ArrowUp`, `Tab`, …) anywhere.
    pub fn press_key(&mut self, key: Key) -> Result<()> {
        let root = self.content;
        self.dispatch(root, DomEventData::KeyDown(Self::key_event(key)))
    }

    /// Close the window: the program gets `close` and should end.
    pub fn close(&mut self) {
        self.events.send(UiEvent::close());
        self.settle();
    }

    /// For tests that want the underlying document.
    pub fn document(&self) -> &BaseDocument {
        &self.doc
    }
}

impl Drop for HeadlessApp {
    fn drop(&mut self) {
        // Unblock a program still waiting for an event so its thread ends.
        self.events.send(UiEvent::close());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
