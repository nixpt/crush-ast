//! The output and failure contract of an inline-lowered polyglot block
//! (crush-ast #312), so `PolyglotMode::Lower` prints what `EXEC_LANG` prints.
//!
//! An `EXEC_LANG` block contributes [`visible_text`] of its stdout — sentinel
//! lines dropped, the rest trimmed — with no trailing newline, and a block that
//! fails is a fatal [`VmError::LangRuntimeError`] whose partial output is
//! dropped. The lowering brackets the call of a lowered block with
//! `lang.capture_begin(lang, line)` and `lang.capture_end()`:
//!
//! - while a thread has a capture open, its output goes into the capture's
//!   buffer ([`emit`]), not the program's output (and so is not streamed early);
//!   other threads print as usual;
//! - `lang.capture_end()` emits [`visible_text`] of the buffer;
//! - a `THROW` that would unwind out of an open capture (to a `try` outside the
//!   block, or to none) is the block failing: [`escaping_throw`] turns it into
//!   the same error `EXEC_LANG` raises, and the partial output is dropped.
//!
//! Both VMs (scheduler and portable) call into this module, so they cannot
//! drift on it.

use crate::vm::{Value, VmError};

/// An open `lang.capture_begin`: the block it belongs to, the call depth of
/// the frame that opened it, and what the block has printed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputCapture {
    pub lang: String,
    pub line: Option<u32>,
    pub call_depth: usize,
    pub buf: String,
}

/// What a polyglot block's stdout contributes to the program's output: every
/// line but the result-sentinel ones, joined by `\n`, trimmed. Shared with
/// [`crate::scheduler::parse_exec_lang_stdout`], so the two modes agree.
pub fn visible_text(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.starts_with(crate::scheduler::CRUSH_RESULT_SENTINEL))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Route one piece of output: into the innermost open capture, if any, else to
/// the program's output.
pub fn emit(captures: &mut [OutputCapture], out_parts: &mut Vec<String>, part: String) {
    match captures.last_mut() {
        Some(capture) => capture.buf.push_str(&part),
        None => out_parts.push(part),
    }
}

/// `lang.capture_begin(lang, line)`, opened by the frame at `call_depth`.
pub fn begin(captures: &mut Vec<OutputCapture>, args: &[Value], call_depth: usize) -> Result<Option<Value>, VmError> {
    let lang = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => return Err(VmError::Unsupported("lang.capture_begin(lang, line): lang must be a string".into())),
    };
    let line = match args.get(1) {
        Some(Value::Int(n)) if *n > 0 => u32::try_from(*n).ok(),
        _ => None,
    };
    captures.push(OutputCapture { lang, line, call_depth, buf: String::new() });
    Ok(None)
}

/// `lang.capture_end()`: close the innermost capture and emit what it caught.
pub fn end(captures: &mut Vec<OutputCapture>, out_parts: &mut Vec<String>) -> Result<Option<Value>, VmError> {
    let capture = captures
        .pop()
        .ok_or_else(|| VmError::Unsupported("lang.capture_end() without lang.capture_begin()".into()))?;
    let text = visible_text(&capture.buf);
    if !text.is_empty() {
        emit(captures, out_parts, text);
    }
    Ok(None)
}

/// A `THROW` of `thrown` that will unwind to a `try` entered at
/// `handler_depth` (`None`: no `try`, so it is uncaught). If that leaves an
/// open capture, the block failed: drop the captures it leaves and return the
/// `EXEC_LANG`-shaped error for the outermost one.
pub fn escaping_throw(
    captures: &mut Vec<OutputCapture>,
    handler_depth: Option<usize>,
    thrown: &Value,
) -> Option<VmError> {
    let first_escaped = captures
        .iter()
        .position(|c| handler_depth.is_none_or(|depth| depth <= c.call_depth))?;
    let capture = captures.drain(first_escaped..).next()?;
    Some(crate::scheduler::lang_runtime_error(
        &capture.lang,
        guest_message(thrown).as_bytes(),
        capture.line,
    ))
}

/// A thrown value as its guest's last error line would read: `ValueError: x`
/// for a lowered Python exception, the value's text otherwise.
fn guest_message(thrown: &Value) -> String {
    if let Value::Map(map) = thrown {
        let map = map.borrow();
        if let Some(ty) = map.get("__exc_type__") {
            let ty = ty.as_text();
            return match map.get("message").map(|m| m.as_text()) {
                Some(message) if !message.is_empty() => format!("{ty}: {message}"),
                _ => ty,
            };
        }
    }
    crate::portable_vm::value_to_text(thrown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_text_is_exec_langs_rule() {
        assert_eq!(visible_text("two\n"), "two");
        assert_eq!(visible_text("\n  a\nb  \n\n"), "a\nb");
        let sentinel = crate::scheduler::CRUSH_RESULT_SENTINEL;
        assert_eq!(visible_text(&format!("x\n{sentinel}{{}}\ny\n")), "x\ny");
    }

    #[test]
    fn output_inside_a_capture_is_held_and_trimmed_on_end() {
        let (mut caps, mut out) = (Vec::new(), Vec::new());
        emit(&mut caps, &mut out, "one\n".into());
        begin(&mut caps, &[Value::Str("python".into()), Value::Int(3)], 1).unwrap();
        emit(&mut caps, &mut out, "two\n".into());
        assert_eq!(out, vec!["one\n"], "held while the capture is open");
        end(&mut caps, &mut out).unwrap();
        emit(&mut caps, &mut out, "three\n".into());
        assert_eq!(out.concat(), "one\ntwothree\n");
    }

    #[test]
    fn a_throw_caught_inside_the_block_does_not_escape() {
        let mut caps = Vec::new();
        begin(&mut caps, &[Value::Str("python".into()), Value::Int(3)], 1).unwrap();
        assert!(escaping_throw(&mut caps, Some(2), &Value::Null).is_none());
        assert_eq!(caps.len(), 1);
    }

    #[test]
    fn a_throw_past_the_block_is_its_failure() {
        let mut caps = Vec::new();
        begin(&mut caps, &[Value::Str("python".into()), Value::Int(3)], 1).unwrap();
        let exc = Value::new_map([
            ("__exc_type__".to_string(), Value::Str("ValueError".into())),
            ("message".to_string(), Value::Str("x".into())),
        ]);
        let err = escaping_throw(&mut caps, Some(1), &exc).expect("escapes");
        assert!(caps.is_empty());
        assert_eq!(err.to_string(), crate::scheduler::lang_runtime_error("python", b"ValueError: x", Some(3)).to_string());
        begin(&mut caps, &[Value::Str("python".into()), Value::Int(3)], 1).unwrap();
        assert!(escaping_throw(&mut caps, None, &Value::Null).is_some(), "uncaught");
    }
}
