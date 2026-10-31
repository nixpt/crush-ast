//! The desktop backend: a window drawn by arniko (bliss + vello + winit).
//!
//! [`run_window`] must be called on the **main thread** (winit requires it)
//! and returns when the window is closed. The Crush program runs on its own
//! thread behind a [`link`](crate::backend::link); the two only talk through
//! channels:
//!
//! ```text
//!  program thread                          main thread (winit loop)
//!  ui.render(html) --Request::Render-->  pump thread -> Signal<String> -> wake
//!                                         reactor flush -> set_inner_html -> rewire
//!  ui.next_event() <----UiEvent--------  DOM click/input/key handlers
//! ```
//!
//! The window opens on the program's first `ui.render`, so `ui.title` before
//! it names the window; the title is read once, when the window is created.
//!
//! **Not exercised on a screen** by this crate's tests (see the README): the
//! page-to-DOM-to-event logic is shared with, and tested by, the headless
//! backend, but `launch_reactive`'s window and renderer need a display.

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use arniko::ARNIKO_STYLES;
use arniko::reactive::{Signal, event_router, launch_reactive};
use bliss::dom::{Attribute, QualName, local_name, ns};
use parking_lot::Mutex;

use crate::backend::{Request, SurfaceEnd};
use crate::event::UiEvent;
use crate::page::Wiring;

/// Is there a display server to open a window on?
fn display_available() -> bool {
    !cfg!(target_os = "linux")
        || ["DISPLAY", "WAYLAND_DISPLAY"]
            .iter()
            .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

/// Show the program's pages in a window until it is closed, then send the
/// program a `close` event. Returns `Ok` without opening anything if the
/// program ends before it ever renders.
pub fn run_window(surface: SurfaceEnd) -> Result<()> {
    let SurfaceEnd { requests, events } = surface;

    // Wait for the first page; `ui.title` calls before it name the window.
    let mut title = String::from("Crush");
    let first = loop {
        match requests.recv() {
            Ok(Request::Title(t)) => title = t,
            Ok(Request::NextEvent) => {}
            Ok(Request::Render(markup)) => break markup,
            Err(_) => return Ok(()),
        }
    };
    if !display_available() {
        events.send(UiEvent::close());
        bail!(
            "cannot open a window: no display (neither DISPLAY nor WAYLAND_DISPLAY is set); \
             run this where a desktop session exists"
        );
    }

    let close_tx = events.clone();
    let wiring = Arc::new(Wiring::new(events));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        launch_reactive(move |m, reactor, router, root, rt| {
            // The setup closure owns the only `&mut EventRouter`, but the
            // page has to be re-wired on every later render. Take the real
            // router (it is the one linked to the document's event sink)
            // and leave a detached one behind.
            let real = std::mem::replace(router, event_router().0);
            let router = Arc::new(Mutex::new(real));
            wiring.install_global_keys(&router.lock());

            let element = |m: &mut bliss::dom::DocumentMutator, tag, attrs| {
                m.create_element(QualName::new(None, ns!(html), tag), attrs)
            };
            if let Ok(Some(head)) = m.doc.query_selector("head") {
                let t = element(m, local_name!("title"), vec![]);
                let text = m.create_text_node(&title);
                m.append_children(t, &[text]);
                m.append_children(head, &[t]);
            }
            let style = element(m, local_name!("style"), vec![]);
            let css = m.create_text_node(ARNIKO_STYLES);
            m.append_children(style, &[css]);
            m.append_children(root, &[style]);
            let content = element(
                m,
                local_name!("div"),
                vec![Attribute {
                    name: QualName::new(None, ns!(), local_name!("id")),
                    value: "crush-ui-content".into(),
                }],
            );
            m.append_children(root, &[content]);
            m.set_inner_html(content, &first);
            wiring.rewire(&router.lock(), m.doc, content);

            // Pages arrive on the signal; the reactor applies each one on
            // the event loop's thread.
            let page = Signal::new(first);
            rt.wire(&page);
            let scope = {
                let (wiring, router) = (wiring.clone(), router.clone());
                reactor.bind_scoped(page.clone(), move |m, html: &String| {
                    m.set_inner_html(content, html);
                    wiring.rewire(&router.lock(), m.doc, content);
                })
            };
            reactor.park_scope(scope);

            std::thread::spawn(move || {
                for request in requests {
                    if let Request::Render(markup) = request {
                        page.set(markup);
                    }
                }
            });
        });
    }));
    // The window is gone (closed, or never came up): the program's loop ends.
    close_tx.send(UiEvent::close());
    outcome.map_err(|_| anyhow!("the window system failed to start (is a display available?)"))
}
