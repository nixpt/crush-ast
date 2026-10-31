//! The seam between the `ui.*` capabilities and whatever draws the page.
//!
//! A [`UiBackend`] is what the program's thread talks to. The desktop and
//! headless backends both use [`link`]: the program thread gets a
//! [`Link`] (a `UiBackend`) and the thread that owns the window — or, in
//! tests, the test — gets the matching [`SurfaceEnd`] and answers it. A
//! terminal backend (phase 2) can do the same, or implement `UiBackend`
//! directly if it has no thread of its own.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::event::UiEvent;

/// What a program asked the surface to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Replace the page with this markup.
    Render(String),
    /// Set the window title.
    Title(String),
    /// The program is blocked in `ui.next_event()` and has nothing more to
    /// draw until it gets one. Surfaces may ignore this; the headless
    /// backend uses it to know a program has finished reacting.
    NextEvent,
}

/// The program's side. Called from the thread running the Crush program;
/// every method may block.
pub trait UiBackend: Send + Sync + 'static {
    /// Replace the whole page with `markup`.
    fn render(&self, markup: &str) -> Result<(), String>;

    /// Set the window title.
    fn set_title(&self, title: &str) -> Result<(), String>;

    /// Block until the next event. After the surface is gone this returns a
    /// `close` event — every time, never an error — so a loop that only
    /// checks for `close` always ends.
    fn next_event(&self) -> UiEvent;
}

/// The program's end of a [`link`].
pub struct Link {
    requests: Mutex<Sender<Request>>,
    events: Mutex<Receiver<UiEvent>>,
    /// Set once a `close` has been delivered: it then repeats forever.
    closed: AtomicBool,
}

/// The surface's end of a [`link`]: requests arrive on `requests`, events
/// go out through `events`. Dropping it closes the program's loop.
pub struct SurfaceEnd {
    pub requests: Receiver<Request>,
    pub events: EventTx,
}

/// The surface's way to deliver events; clones share one count.
#[derive(Clone)]
pub struct EventTx {
    tx: Sender<UiEvent>,
    sent: Arc<AtomicUsize>,
}

impl EventTx {
    /// Deliver `event` to the program (dropped if the program has gone).
    pub fn send(&self, event: UiEvent) {
        self.sent.fetch_add(1, Ordering::SeqCst);
        let _ = self.tx.send(event);
    }

    /// How many events have been sent so far. A surface that needs to know
    /// whether an input produced one (the headless backend) compares this.
    pub fn sent(&self) -> usize {
        self.sent.load(Ordering::SeqCst)
    }
}

/// A connected pair: the program's [`Link`] and the surface's [`SurfaceEnd`].
pub fn link() -> (Link, SurfaceEnd) {
    let (req_tx, req_rx) = mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    (
        Link {
            requests: Mutex::new(req_tx),
            events: Mutex::new(ev_rx),
            closed: AtomicBool::new(false),
        },
        SurfaceEnd {
            requests: req_rx,
            events: EventTx {
                tx: ev_tx,
                sent: Arc::new(AtomicUsize::new(0)),
            },
        },
    )
}

impl Link {
    /// A request to a surface that has gone is dropped, not an error: the
    /// program learns about it from the `close` event, which is the one
    /// place it is documented to look.
    fn send(&self, request: Request) -> Result<(), String> {
        let requests = self
            .requests
            .lock()
            .map_err(|_| "ui: backend lock poisoned".to_string())?;
        let _ = requests.send(request);
        Ok(())
    }
}

impl UiBackend for Link {
    fn render(&self, markup: &str) -> Result<(), String> {
        self.send(Request::Render(markup.to_string()))
    }

    fn set_title(&self, title: &str) -> Result<(), String> {
        self.send(Request::Title(title.to_string()))
    }

    fn next_event(&self) -> UiEvent {
        if self.closed.load(Ordering::SeqCst) {
            return UiEvent::close();
        }
        let _ = self.send(Request::NextEvent);
        let event = match self.events.lock() {
            Ok(events) => events.recv().unwrap_or_else(|_| UiEvent::close()),
            Err(_) => UiEvent::close(),
        };
        if event.is_close() {
            self.closed.store(true, Ordering::SeqCst);
        }
        event
    }
}
