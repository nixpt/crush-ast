//! Debug events for hosts that embed a debugger (an IDE adapter, a shell,
//! a browser playground) instead of driving the REPL (CRUSH-160).
//!
//! A [`DebugSession`](crate::DebugSession) emits one event per thing that
//! happened during a command: program output, then why execution stopped
//! with a snapshot of every frame. Values in events are
//! [`ValueView`]s rendered at the session's grant-derived visibility, never
//! raw VM values, so a sink can't see more than the grants allow. Events
//! are `Send`; a channel sender is a sink.

use std::sync::{Arc, Mutex, mpsc};

use crush_vm::{FrameSnapshot, ValueView, WatchId};

use crate::breakpoint::BreakpointId;

/// Why execution stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Breakpoint(BreakpointId),
    /// A `step`/`next`/`finish` completed.
    Step,
    /// The last instruction changed a watched slot.
    Watchpoint {
        id: WatchId,
        slot: u16,
        depth: usize,
        /// `None` when the slot had never been assigned in that frame.
        old: Option<ValueView>,
        new: ValueView,
    },
    /// Paused for another reason (e.g. waiting on host input).
    Paused,
}

/// Something a debug session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugEvent {
    /// Execution paused. `frames` runs from the innermost frame outwards.
    Stopped {
        reason: StopReason,
        frames: Vec<FrameSnapshot>,
    },
    /// Text the program printed since the last event.
    Output(String),
    /// The program finished.
    Finished,
    /// A quota (steps, stack, output, call depth) stopped the program.
    QuotaExceeded(usize),
    /// The program failed with a runtime error.
    Error(String),
    /// The session refused an operation its grants don't allow.
    Refused {
        operation: &'static str,
        needs: &'static str,
    },
}

/// Receives a session's events.
pub trait DebugEventSink {
    fn emit(&mut self, event: DebugEvent);
}

impl DebugEventSink for mpsc::Sender<DebugEvent> {
    /// A disconnected receiver drops the event; the session keeps running.
    fn emit(&mut self, event: DebugEvent) {
        let _ = self.send(event);
    }
}

/// Keeps every event; clones share the same list, so keep one clone to
/// read what the session (which owns the other) emitted.
#[derive(Debug, Clone, Default)]
pub struct CollectingSink {
    events: Arc<Mutex<Vec<DebugEvent>>>,
}

impl CollectingSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything emitted so far.
    pub fn events(&self) -> Vec<DebugEvent> {
        self.events.lock().expect("sink lock poisoned").clone()
    }
}

impl DebugEventSink for CollectingSink {
    fn emit(&mut self, event: DebugEvent) {
        self.events.lock().expect("sink lock poisoned").push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collecting_sink_clones_share_events() {
        let sink = CollectingSink::new();
        let mut writer = sink.clone();
        writer.emit(DebugEvent::Finished);
        assert_eq!(sink.events(), vec![DebugEvent::Finished]);
    }

    #[test]
    fn channel_sender_is_a_sink_and_events_cross_threads() {
        let (mut tx, rx) = mpsc::channel();
        std::thread::spawn(move || tx.emit(DebugEvent::Output("hi\n".into())))
            .join()
            .unwrap();
        assert_eq!(rx.recv().unwrap(), DebugEvent::Output("hi\n".into()));
    }

    #[test]
    fn a_dropped_receiver_does_not_panic_the_session() {
        let (mut tx, rx) = mpsc::channel::<DebugEvent>();
        drop(rx);
        tx.emit(DebugEvent::Finished);
    }
}
