//! Machine-readable debugging: [`DebugEvent`]s as NDJSON, one object per
//! line, for an editor or agent that drives the debugger over
//! stdin/stdout (`DebugSession::run_ndjson`, `crush debug --message-format
//! json`).
//!
//! Two kinds of line share the stream, told apart by their keys:
//!
//! - **events**, with an `"event"` key: `started`, `stopped`, `output`,
//!   `finished`, `quota_exceeded`, `error`, `refused` and `message` (the
//!   answer to a command that is not a stop: `break`, `print`, `list`, ...);
//! - **diagnostics**, in `crush-diagnostics`' record format (`code`, `level`,
//!   `message`, `hint`): a command line that did not parse or a command that
//!   failed. Codes: `E-DBG01` (bad command), `E-DBG02` (command failed).
//!
//! Values are never raw: each is a `ValueView` rendered at the session's
//! grant-derived visibility — `{"hidden":true}`, `{"type":"int","hash":"…"}`
//! (redacted) or `{"type":"int","text":"5"}` (full).

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crush_vm::{FrameSnapshot, ValueView};

use crate::events::{DebugEvent, DebugEventSink, StopReason};

/// What the sink needs to put source positions and names on events.
#[derive(Clone, Default)]
pub struct Locator {
    /// `(source line, bytecode offset)`, as the session holds it.
    pub source_map: Vec<(usize, usize)>,
    /// The source file's name, if the program was compiled from one.
    pub file: Option<String>,
    /// `function -> [(name, slot)]`.
    pub local_names: HashMap<String, Vec<(String, u16)>>,
    /// The source text, one entry per line.
    pub source_lines: Vec<String>,
    /// How a refusal tells the user to grant `debug.*`.
    pub grant_hint: Option<fn(&str) -> String>,
}

impl Locator {
    fn line_for_ip(&self, ip: usize) -> Option<usize> {
        self.source_map
            .iter()
            .filter(|&&(_, offset)| offset <= ip)
            .max_by_key(|&&(_, offset)| offset)
            .map(|&(line, _)| line)
    }

    fn name_of(&self, function: Option<&str>, slot: u16) -> Option<&str> {
        self.local_names
            .get(function?)?
            .iter()
            .find(|(_, s)| *s == slot)
            .map(|(n, _)| n.as_str())
    }
}

/// A [`ValueView`] as JSON.
pub fn value_json(view: &ValueView) -> Value {
    match view {
        ValueView::Hidden => json!({ "hidden": true }),
        ValueView::Redacted { type_name, hash } => {
            json!({ "type": type_name, "hash": format!("{hash:016x}") })
        }
        ValueView::Plain { type_name, text } => json!({ "type": type_name, "text": text }),
    }
}

fn frame_json(frame: &FrameSnapshot, at: &Locator) -> Value {
    let mut v = json!({
        "depth": frame.depth,
        "function": frame.function,
        "ip": frame.ip,
        "locals": frame.locals.iter().map(|(slot, view)| {
            let mut l = json!({ "slot": slot, "value": value_json(view) });
            if let Some(name) = at.name_of(frame.function.as_deref(), *slot) {
                l["name"] = json!(name);
            }
            l
        }).collect::<Vec<_>>(),
    });
    if let Some(line) = at.line_for_ip(frame.ip) {
        v["line"] = json!(line);
        v["file"] = json!(at.file);
    }
    v
}

/// One event as its NDJSON object.
pub fn event_json(event: &DebugEvent, at: &Locator) -> Value {
    match event {
        DebugEvent::Stopped { reason, frames } => {
            let reason = match reason {
                StopReason::Breakpoint(id) => json!({ "kind": "breakpoint", "id": id.0 }),
                StopReason::Step => json!({ "kind": "step" }),
                StopReason::Paused => json!({ "kind": "paused" }),
                StopReason::Watchpoint {
                    id,
                    slot,
                    depth,
                    old,
                    new,
                } => {
                    // The watched frame, to name the slot.
                    let function = frames
                        .iter()
                        .find(|f| f.depth == *depth)
                        .and_then(|f| f.function.as_deref());
                    let mut r = json!({
                        "kind": "watchpoint", "id": id.0, "slot": slot, "depth": depth,
                        "old": old.as_ref().map(value_json), "new": value_json(new),
                    });
                    if let Some(name) = at.name_of(function, *slot) {
                        r["name"] = json!(name);
                    }
                    r
                }
            };
            let mut v = json!({
                "event": "stopped",
                "reason": reason,
                "frames": frames.iter().map(|f| frame_json(f, at)).collect::<Vec<_>>(),
            });
            if let Some(top) = frames.first() {
                v["function"] = json!(top.function);
                if let Some(line) = at.line_for_ip(top.ip) {
                    v["file"] = json!(at.file);
                    v["line"] = json!(line);
                    if let Some(src) = at.source_lines.get(line - 1) {
                        v["source"] = json!(src);
                    }
                }
            }
            v
        }
        DebugEvent::Output(text) => json!({ "event": "output", "text": text }),
        DebugEvent::Finished => json!({ "event": "finished" }),
        DebugEvent::QuotaExceeded(n) => json!({ "event": "quota_exceeded", "limit": n }),
        DebugEvent::Error(message) => json!({ "event": "error", "message": message }),
        DebugEvent::Refused { operation, needs } => {
            let mut v = json!({ "event": "refused", "operation": operation, "needs": needs });
            if let Some(hint) = at.grant_hint {
                v["hint"] = json!(hint(needs));
            }
            v
        }
    }
}

/// A [`DebugEventSink`] that writes each event as one NDJSON line.
pub struct NdjsonSink<W: Write + Send> {
    out: Arc<Mutex<W>>,
    at: Locator,
    emitted: Arc<AtomicUsize>,
}

impl<W: Write + Send> NdjsonSink<W> {
    pub fn new(out: Arc<Mutex<W>>, at: Locator, emitted: Arc<AtomicUsize>) -> Self {
        Self { out, at, emitted }
    }
}

impl<W: Write + Send> DebugEventSink for NdjsonSink<W> {
    fn emit(&mut self, event: DebugEvent) {
        self.emitted.fetch_add(1, Ordering::SeqCst);
        write_line(&self.out, &event_json(&event, &self.at));
    }
}

/// Write `value` and a newline, then flush, so a reader on a pipe sees the
/// line now. A closed pipe is the reader's business, not the session's.
pub(crate) fn write_line<W: Write>(out: &Mutex<W>, value: &Value) {
    let mut out = out.lock().expect("ndjson writer poisoned");
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

/// A `crush-diagnostics` record line (no trailing newline) for a command
/// that failed.
pub(crate) fn diag(code: &str, message: &str, hint: Option<&str>) -> Value {
    let line = crush_diagnostics::diag_line_from(code, "error", message, hint, None);
    serde_json::from_str(line.trim_end()).expect("diag_line_from emits JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_never_carry_more_than_their_view() {
        assert_eq!(value_json(&ValueView::Hidden), json!({ "hidden": true }));
        let redacted = ValueView::Redacted {
            type_name: "int",
            hash: 255,
        };
        assert_eq!(
            value_json(&redacted),
            json!({ "type": "int", "hash": "00000000000000ff" })
        );
        let plain = ValueView::Plain {
            type_name: "int",
            text: "5".into(),
        };
        assert_eq!(value_json(&plain), json!({ "type": "int", "text": "5" }));
    }

    #[test]
    fn a_stop_names_its_line_function_and_locals() {
        let at = Locator {
            source_map: vec![(3, 0), (4, 10)],
            file: Some("a.crush".into()),
            local_names: HashMap::from([("main".to_string(), vec![("x".to_string(), 0)])]),
            source_lines: vec!["a".into(), "b".into(), "let x = 1".into(), "x + 1".into()],
            grant_hint: None,
        };
        let frame = FrameSnapshot {
            depth: 1,
            function: Some("main".into()),
            ip: 12,
            locals: vec![(
                0,
                ValueView::Plain {
                    type_name: "int",
                    text: "7".into(),
                },
            )],
        };
        let v = event_json(
            &DebugEvent::Stopped {
                reason: StopReason::Step,
                frames: vec![frame],
            },
            &at,
        );
        assert_eq!(v["event"], "stopped");
        assert_eq!(v["line"], 4);
        assert_eq!(v["file"], "a.crush");
        assert_eq!(v["source"], "x + 1");
        assert_eq!(v["frames"][0]["locals"][0]["name"], "x");
        assert_eq!(v["frames"][0]["locals"][0]["value"]["text"], "7");
    }

    #[test]
    fn a_refusal_names_the_flag() {
        let at = Locator {
            grant_hint: Some(|g| format!("pass --grant {g}")),
            ..Locator::default()
        };
        let v = event_json(
            &DebugEvent::Refused {
                operation: "step",
                needs: "debug.step",
            },
            &at,
        );
        assert_eq!(v["hint"], "pass --grant debug.step");
    }

    #[test]
    fn diag_records_use_the_diagnostics_format() {
        let v = diag("E-DBG01", "unknown command `x`", Some("try help"));
        assert_eq!(v["code"], "E-DBG01");
        assert_eq!(v["level"], "error");
        assert_eq!(v["hint"], "try help");
    }
}
