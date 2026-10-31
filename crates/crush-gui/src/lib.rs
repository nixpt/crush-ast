//! `crush-gui`: a grant-gated `ui.*` capability API for Crush programs.
//!
//! A program renders markup with `ui.render` and *pulls* events with a
//! blocking `ui.next_event()`; nothing calls back into the VM. See
//! `docs/design/ui-api.md` for the API and `README.md` for how to run it.
//!
//! * [`caps`] - the `ui.*` capabilities and the `--ui` grant.
//! * [`backend`] - [`UiBackend`], the seam every backend implements.
//! * [`desktop`] - the arniko window.
//! * [`headless`] - a window-less DOM for tests.
//! * [`run`] - compile, preflight and run a program.

pub mod backend;
pub mod caps;
pub mod components;
pub mod desktop;
pub mod event;
pub mod headless;
mod page;
pub mod run;

pub use backend::{EventTx, Link, Request, SurfaceEnd, UiBackend, link};
pub use event::UiEvent;
pub use run::{Grants, NotGranted, RunEnd, Source, execute};
