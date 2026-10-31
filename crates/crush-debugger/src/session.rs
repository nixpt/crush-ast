//! Debug session: owns the VM driver + breakpoint registry +
//! assembler sourcemap.
//!
//! The session resolves `file:line` breakpoints to bytecode offsets
//! via the sourcemap at add-time, so the VM driver can forward them
//! to `PortableVm` for native pause-on-hit.

use std::collections::HashMap;
use std::marker::PhantomData;

use super::breakpoint::{BreakpointId, BreakpointSet};
use super::events::{DebugEvent, DebugEventSink, StopReason};
use super::repl::Command;
use super::vm_driver::{VmDriver, VmRunResult};

/// Returns the quota value `N` if `e` is any of the four quota
/// exhaustion variants, or `None` otherwise.
fn quota_n(e: &crush_vm::VmError) -> Option<usize> {
    match e {
        crush_vm::VmError::StepQuota(n)
        | crush_vm::VmError::StackQuota(n)
        | crush_vm::VmError::OutputQuota(n)
        | crush_vm::VmError::CallDepthQuota(n) => Some(*n),
        _ => None,
    }
}

/// Formats a quota error as a clean `"quota exceeded (N)"` message.
/// Caller must ensure `e` is a quota variant.
fn quota_message(e: &crush_vm::VmError) -> String {
    let n = quota_n(e).expect("quota_message called on non-quota error");
    format!("quota exceeded ({n})")
}

/// The field `name` of a record or map, or the element of an array or tuple
/// at index `name`; boxed variables are looked through.
fn member(value: &crush_vm::vm::Value, name: &str) -> Option<crush_vm::vm::Value> {
    use crush_vm::vm::Value;
    match value {
        Value::Cell(c) => member(&c.borrow(), name),
        Value::Record(r) => r.field(name),
        Value::Map(m) => m.borrow().get(name).cloned(),
        Value::Array(a) | Value::Vector(a) => a.borrow().get(name.parse::<usize>().ok()?).cloned(),
        Value::Tuple(t) => t.get(name.parse::<usize>().ok()?).cloned(),
        _ => None,
    }
}

/// REPL help text shown on `help` / `h` / `?`.
const HELP_BANNER: &str = "\
Commands:
  break <file>:<line> | <line> — set a breakpoint
  delete <id>          — remove a breakpoint
  step | s             — execute one instruction, or one source line (enters calls)
  next | n             — step over: run a call to completion
  stepi | nexti        — the same, always by instruction
  finish | fin         — step out: run until this function returns
  continue | c         — run until breakpoint, watchpoint or done
  watch <slot|name> [top | frame <depth>]
                       — stop when a local changes (default: this frame)
  unwatch <id>         — remove a watchpoint
  list | l             — list all breakpoints
  status | info | i    — show VM state
  print <slot|name[.field]> | p — print a local, or a field of it
  locals | vars        — print every assigned local of the current frame
  quit | q             — exit
  help | h | ?         — show this help";

/// The lifetime parameter `'a` mirrors the VM borrow; the `D: VmDriver`
/// parameter lets tests swap a mock driver.
pub struct DebugSession<'a, D: VmDriver> {
    driver: D,
    breakpoints: BreakpointSet,
    /// Assembler sourcemap: `(source_line, bytecode_offset)` for
    /// resolving `file:line` breakpoints to VM addresses.
    source_map: Vec<(usize, usize)>,
    /// Name of the source file the program was compiled from, if the host
    /// compiled one (`set_source_name`): stops then say where they are.
    source_name: Option<String>,
    /// Local variable names per function: `function -> [(name, slot)]`.
    local_names: HashMap<String, Vec<(String, u16)>>,
    /// Whether `step`/`next` go by source line (`set_line_stepping`).
    line_stepping: bool,
    /// The source text, one entry per line, to show at stops.
    source_lines: Vec<String>,
    /// Phrases the flag that grants `debug.*` capability, for refusals.
    grant_hint: fn(&str) -> String,
    /// Renders values at the grant-derived visibility (fixed at `new`).
    redactor: crush_vm::Redactor,
    /// Where events go, if a host attached a sink.
    sink: Option<Box<dyn DebugEventSink>>,
    /// Tie the session's lifetime to the VM's; without it the `'a`
    /// would be unused and Rust would reject the struct.
    _vm_ref: PhantomData<&'a ()>,
}

impl<'a, D: VmDriver> DebugSession<'a, D> {
    /// Construct a session around an already-attached VM driver.
    /// `source_map` is the assembler line→offset mapping; pass
    /// `Vec::new()` for programs without debug info.
    ///
    /// What the session may do is read once, here, from the driver's
    /// `debug.*` grants (`VmDriver::debug_visibility`).
    pub fn new(driver: D, source_map: Vec<(usize, usize)>) -> Self {
        let redactor = crush_vm::Redactor::new(driver.debug_visibility());
        Self {
            driver,
            breakpoints: BreakpointSet::new(),
            source_map,
            source_name: None,
            local_names: HashMap::new(),
            line_stepping: false,
            source_lines: Vec::new(),
            grant_hint: |grant| format!("crush-debugger run --cap {grant}"),
            redactor,
            sink: None,
            _vm_ref: PhantomData,
        }
    }

    /// Say which source file the program came from, so stops report
    /// `file:line` (needs a source map of source lines, as
    /// `crush-lang-sdk` builds for `.crush` files).
    pub fn set_source_name(&mut self, name: impl Into<String>) {
        self.source_name = Some(name.into());
    }

    /// Make `step` and `next` run to the next source line instead of the
    /// next instruction (`stepi`/`nexti` stay by instruction). Needs a source
    /// map of source lines; without one they keep going by instruction.
    pub fn set_line_stepping(&mut self, on: bool) {
        self.line_stepping = on;
    }

    /// The program's source text, to show the current line at each stop.
    pub fn set_source_text(&mut self, text: &str) {
        self.source_lines = text.lines().map(str::to_string).collect();
    }

    /// Give the program's local names, `function -> [(name, slot)]`, so
    /// `print`/`watch` take names and `locals` labels its output.
    pub fn set_local_names(&mut self, names: HashMap<String, Vec<(String, u16)>>) {
        self.local_names = names;
    }

    /// How a refusal tells the user to grant `debug.*` (default: the
    /// `crush-debugger run --cap` spelling). Receives the grant name.
    pub fn set_grant_hint(&mut self, hint: fn(&str) -> String) {
        self.grant_hint = hint;
    }

    /// The source line whose code contains bytecode offset `ip`: the last
    /// source-map entry at or before it. `None` without a source map.
    pub fn line_for_ip(&self, ip: usize) -> Option<usize> {
        self.source_map
            .iter()
            .filter(|&&(_, offset)| offset <= ip)
            .max_by_key(|&&(_, offset)| offset)
            .map(|&(line, _)| line)
    }

    /// The visibility this session's grants allow.
    pub fn visibility(&self) -> crush_vm::DebugVisibility {
        self.redactor.visibility()
    }

    /// Send this session's events to `sink` (replacing any previous one).
    pub fn set_event_sink(&mut self, sink: impl DebugEventSink + 'static) {
        self.sink = Some(Box::new(sink));
    }

    fn emit(&mut self, event: DebugEvent) {
        if let Some(sink) = self.sink.as_mut() {
            sink.emit(event);
        }
    }

    /// Register a source-level breakpoint and forward the updated set
    /// to the underlying driver. Resolves `file:line` to a bytecode
    /// offset via the source map if available. Returns the first id; see
    /// [`add_breakpoints`](Self::add_breakpoints) when a line has code in
    /// more than one function.
    pub fn add_breakpoint(
        &mut self,
        file: impl Into<std::path::PathBuf>,
        line: u32,
    ) -> BreakpointId {
        self.add_breakpoints(file, line)[0]
    }

    /// Like [`add_breakpoint`](Self::add_breakpoint), but a line that has
    /// code in several functions (`let f = |x| x * k` is code in the
    /// enclosing function and in the closure) gets one breakpoint, with its
    /// own id, in each. Always returns at least one id.
    pub fn add_breakpoints(
        &mut self,
        file: impl Into<std::path::PathBuf>,
        line: u32,
    ) -> Vec<BreakpointId> {
        let file = file.into();
        let addrs = self.resolve_line(line);
        if addrs.is_empty() && !self.source_map.is_empty() {
            let next = match self.next_line_with_code(line) {
                Some(n) => format!("; the next line with code is {n}"),
                None => String::new(),
            };
            eprintln!(
                "warning: breakpoint at {}:{} has no matching bytecode address \
                 (line not in sourcemap){next}",
                file.display(),
                line
            );
        }
        let ids: Vec<BreakpointId> = if addrs.is_empty() {
            vec![self.breakpoints.add(file, line, None)]
        } else {
            addrs
                .into_iter()
                .map(|a| self.breakpoints.add(file.clone(), line, Some(a)))
                .collect()
        };
        self.driver.set_breakpoints(&self.breakpoints);
        ids
    }

    /// The bytecode offsets a breakpoint on `line` stops at: one per
    /// function the line has code in (the lowest offset there). Empty when
    /// the line has no code.
    fn resolve_line(&self, line: u32) -> Vec<u32> {
        let line = line as usize;
        let mut offsets: Vec<usize> = self
            .source_map
            .iter()
            .filter(|&&(l, _)| l == line)
            .map(|&(_, o)| o)
            .collect();
        offsets.sort_unstable();
        let mut seen = std::collections::HashSet::new();
        offsets
            .into_iter()
            .filter(|&o| seen.insert(self.driver.function_at(o).unwrap_or_default()))
            .map(|o| o as u32)
            .collect()
    }

    /// The first line after `line` that has code, to tell a user whose
    /// breakpoint landed on a blank line, comment or closing brace.
    fn next_line_with_code(&self, line: u32) -> Option<usize> {
        self.source_map
            .iter()
            .map(|&(l, _)| l)
            .filter(|&l| l > line as usize)
            .min()
    }

    /// Remove a breakpoint by id.
    pub fn remove_breakpoint(&mut self, id: BreakpointId) -> bool {
        let removed = self.breakpoints.remove(id);
        if removed {
            self.driver.set_breakpoints(&self.breakpoints);
        }
        removed
    }

    /// Read-only snapshot count.
    pub fn breakpoint_count(&self) -> usize {
        self.breakpoints.len()
    }

    /// Borrow the local breakpoint set for inspection (REPL `list`).
    pub fn breakpoints(&self) -> &BreakpointSet {
        &self.breakpoints
    }

    /// Run the REPL loop. Reads commands from stdin, parses via
    /// `repl::parse_command`, dispatches to `handle_command`, and
    /// prints results. Exits on EOF or `quit`.
    ///
    /// Breakpoints, steps and watchpoints pause through `PortableVm`'s
    /// own hooks (`VmYield::DebugBreak` + `last_stop`).
    pub fn run_repl(&mut self) -> anyhow::Result<()> {
        let stdin = std::io::stdin();
        self.run_repl_io(stdin.lock(), std::io::stdout())
    }

    /// [`run_repl`](Self::run_repl) over any reader and writer, so a host can
    /// feed it commands ahead of (or instead of) stdin.
    pub fn run_repl_io(
        &mut self,
        mut reader: impl std::io::BufRead,
        mut stdout: impl std::io::Write,
    ) -> anyhow::Result<()> {
        let mut line = String::new();

        loop {
            write!(stdout, "cru-s-debugger> ")?;
            stdout.flush()?;
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                // EOF — quit cleanly
                break;
            }
            match super::repl::parse_command(&line) {
                Ok(super::repl::Command::Quit) => {
                    writeln!(stdout, "bye.")?;
                    break;
                }
                Ok(cmd) => match self.handle_command(cmd) {
                    Ok(Some(msg)) => writeln!(stdout, "{}", msg)?,
                    Ok(None) => {}
                    Err(e) => writeln!(stdout, "error: {}", e)?,
                },
                Err(super::repl::ParseCommandError::Empty) => {
                    // blank line — silent no-op
                }
                Err(e) => writeln!(stdout, "error: {}", e)?,
            }
        }
        Ok(())
    }

    /// Drive the session over NDJSON: read command lines from `input`, write
    /// one JSON object per line to `out` (see [`crate::ndjson`] for the
    /// schema). No prompt and no prose; ends at EOF or `quit`. This is the
    /// mode for an editor or agent on a pipe.
    pub fn run_ndjson<R, W>(&mut self, input: R, out: W) -> anyhow::Result<()>
    where
        R: std::io::BufRead,
        W: std::io::Write + Send + 'static,
    {
        use crate::ndjson::{Locator, NdjsonSink, diag, write_line};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let out = Arc::new(Mutex::new(out));
        let emitted = Arc::new(AtomicUsize::new(0));
        let at = Locator {
            source_map: self.source_map.clone(),
            file: self.source_name.clone(),
            local_names: self.local_names.clone(),
            source_lines: self.source_lines.clone(),
            grant_hint: Some(self.grant_hint),
        };
        self.set_event_sink(NdjsonSink::new(out.clone(), at, emitted.clone()));
        write_line(
            &out,
            &serde_json::json!({
                "event": "started",
                "file": self.source_name,
                "visibility": format!("{:?}", self.visibility()),
            }),
        );
        for line in input.lines() {
            let line = line?;
            let before = emitted.load(Ordering::SeqCst);
            match super::repl::parse_command(&line) {
                Ok(super::repl::Command::Quit) => break,
                Ok(cmd) => match self.handle_command(cmd) {
                    // A command that ran the program already said so with events.
                    Ok(Some(text)) if emitted.load(Ordering::SeqCst) == before => write_line(
                        &out,
                        &serde_json::json!({ "event": "message", "command": line.trim(), "text": text }),
                    ),
                    Ok(_) => {}
                    Err(e) => write_line(&out, &diag("E-DBG02", &e.to_string(), None)),
                },
                Err(super::repl::ParseCommandError::Empty) => {}
                Err(e) => write_line(
                    &out,
                    &diag(
                        "E-DBG01",
                        &e.to_string(),
                        Some("send `help` for the commands"),
                    ),
                ),
            }
        }
        Ok(())
    }

    /// Dispatch a parsed REPL command to the VM driver and breakpoint
    /// registry. Returns `Ok(Some(message))` to print a result line,
    /// `Ok(None)` for a silent no-op. `Quit` is handled by `run_repl`
    /// before this method is called.
    ///
    /// Quota errors (StepQuota, StackQuota, OutputQuota,
    /// CallDepthQuota) are caught and reported as a clean
    /// `"quota exceeded (N)"` message, matching the `continue` path's
    /// `VmRunResult::QuotaExceeded` format.
    ///
    /// Public so an embedding host can drive the session without the
    /// stdin REPL (pair it with `set_event_sink`).
    ///
    /// Grant-gated: a command its grants don't allow is refused with a
    /// message naming the missing grant (and a `Refused` event); nothing
    /// reaches the VM. Program output printed by the command comes first in
    /// the message.
    pub fn handle_command(&mut self, cmd: Command) -> anyhow::Result<Option<String>> {
        if let Some((operation, needs)) = self.missing_grant(&cmd) {
            self.emit(DebugEvent::Refused { operation, needs });
            return Ok(Some(format!(
                "refused: `{operation}` needs the `{needs}` grant \
                 ({})",
                (self.grant_hint)(needs)
            )));
        }
        let message = self.run_command(cmd)?;
        Ok(message)
    }

    /// The grant `cmd` lacks, as `(command, grant)`, or `None` if allowed.
    fn missing_grant(&self, cmd: &Command) -> Option<(&'static str, &'static str)> {
        let operation = match cmd {
            Command::Break { .. } => "break",
            Command::Delete { .. } => "delete",
            Command::Step => "step",
            Command::Next => "next",
            Command::StepInstruction => "stepi",
            Command::NextInstruction => "nexti",
            Command::Finish => "finish",
            Command::Continue => "continue",
            Command::Watch { .. } | Command::WatchName { .. } => "watch",
            Command::Unwatch { .. } => "unwatch",
            Command::Print { .. } | Command::Locals => {
                return (!self.visibility().can_inspect()).then_some((
                    if matches!(cmd, Command::Locals) {
                        "locals"
                    } else {
                        "print"
                    },
                    crush_vm::debug::DEBUG_INSPECT_REDACTED,
                ));
            }
            Command::List | Command::Status | Command::Help | Command::Quit => return None,
        };
        (!self.visibility().can_control()).then_some((operation, crush_vm::debug::DEBUG_STEP))
    }

    fn run_command(&mut self, cmd: Command) -> anyhow::Result<Option<String>> {
        match cmd {
            super::repl::Command::Help => Ok(Some(HELP_BANNER.to_string())),
            super::repl::Command::Step if self.stepping_by_line() => {
                self.run_line_step(crush_vm::StepMode::Into)
            }
            super::repl::Command::Next if self.stepping_by_line() => {
                self.run_line_step(crush_vm::StepMode::Over)
            }
            super::repl::Command::NextInstruction => self.run_step(crush_vm::StepMode::Over),
            super::repl::Command::Step | super::repl::Command::StepInstruction => {
                let stepped = self.driver.step();
                let output = self.drain_output();
                match stepped {
                    Ok(outcome) => {
                        let reason = if outcome.yielded {
                            StopReason::Paused
                        } else {
                            StopReason::Step
                        };
                        self.emit_stop(reason);
                        Ok(Some(format!(
                            "{output}step {}: yielded={}{}",
                            outcome.instruction_count,
                            outcome.yielded,
                            self.where_suffix()
                        )))
                    }
                    Err(super::vm_driver::VmError::Inner(ref e)) if quota_n(e).is_some() => {
                        self.emit(DebugEvent::QuotaExceeded(quota_n(e).unwrap_or(0)));
                        Ok(Some(format!("{output}{}", quota_message(e))))
                    }
                    Err(e) => {
                        self.emit(DebugEvent::Error(e.to_string()));
                        Err(anyhow::anyhow!("{}", e))
                    }
                }
            }
            super::repl::Command::Continue => {
                let result = self.driver.run_until_breakpoint_or_done();
                self.report(result)
            }
            super::repl::Command::List => {
                if self.breakpoints.is_empty() {
                    Ok(Some("no breakpoints".to_string()))
                } else {
                    let lines: Vec<String> = self
                        .breakpoints
                        .list()
                        .iter()
                        .map(|bp| {
                            format!(
                                "#{}: {}:{}",
                                bp.id.0,
                                bp.location.file.display(),
                                bp.location.line
                            )
                        })
                        .collect();
                    Ok(Some(lines.join("\n")))
                }
            }
            super::repl::Command::Break { mut file, line } => {
                // `break 12`: a line of the program being debugged.
                if file.as_os_str().is_empty() {
                    file = self.source_name.clone().unwrap_or_default().into();
                }
                let ids = self.add_breakpoints(file.clone(), line);
                let moved = if !self.source_map.is_empty() && self.resolve_line(line).is_empty() {
                    match self.next_line_with_code(line) {
                        Some(n) => {
                            format!(
                                " (no code on line {line}, so it will not fire; the next line with code is {n})"
                            )
                        }
                        None => format!(" (no code on line {line}, so it will not fire)"),
                    }
                } else {
                    String::new()
                };
                let many = if ids.len() > 1 {
                    format!(" (code in {} functions)", ids.len())
                } else {
                    String::new()
                };
                let ids = ids
                    .iter()
                    .map(|i| format!("#{}", i.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                let noun = if many.is_empty() {
                    "breakpoint"
                } else {
                    "breakpoints"
                };
                Ok(Some(format!(
                    "{noun} {ids} set at {}:{}{moved}{many}",
                    file.display(),
                    line
                )))
            }
            super::repl::Command::Delete { id } => {
                let bp_id = BreakpointId(id);
                if self.remove_breakpoint(bp_id) {
                    Ok(Some(format!("breakpoint #{} removed", id)))
                } else {
                    Ok(Some(format!("no breakpoint #{}", id)))
                }
            }
            super::repl::Command::Next => self.run_step(crush_vm::StepMode::Over),
            super::repl::Command::Finish => self.run_step(crush_vm::StepMode::Out),
            super::repl::Command::WatchName { name, scope } => {
                let frame_depth = match scope {
                    Some(crush_vm::WatchScope::Frame(d)) => d,
                    _ => self.driver.call_depth(),
                };
                match self.slot_named("watch", &name, frame_depth) {
                    Ok(slot) => self.run_command(super::repl::Command::Watch { slot, scope }),
                    Err(msg) => Ok(Some(msg)),
                }
            }
            super::repl::Command::Locals => {
                let depth = self.driver.call_depth();
                let frames = self.driver.frames(&self.redactor);
                let Some(frame) = frames.iter().find(|f| f.depth == depth) else {
                    return Ok(Some("no frame (the program has not started)".into()));
                };
                if frame.locals.is_empty() {
                    return Ok(Some(format!("no locals assigned in frame {depth}")));
                }
                let names = frame
                    .function
                    .as_ref()
                    .and_then(|f| self.local_names.get(f));
                let lines: Vec<String> = frame
                    .locals
                    .iter()
                    .map(|(slot, view)| {
                        match names.and_then(|n| n.iter().find(|(_, s)| s == slot)) {
                            Some((name, _)) => format!("{name} (slot {slot}) = {view}"),
                            None => format!("slot {slot} = {view}"),
                        }
                    })
                    .collect();
                Ok(Some(lines.join("\n")))
            }
            super::repl::Command::Watch { slot, scope } => {
                let depth = self.driver.call_depth();
                let scope = scope.unwrap_or(if depth == 0 {
                    crush_vm::WatchScope::Top
                } else {
                    crush_vm::WatchScope::Frame(depth)
                });
                match self.driver.add_watchpoint(slot, scope) {
                    Some(id) => {
                        let where_ = match scope {
                            crush_vm::WatchScope::Top => "the running frame".to_string(),
                            crush_vm::WatchScope::Frame(d) => format!("frame {d}"),
                        };
                        Ok(Some(format!(
                            "watchpoint #{} set on slot {slot} in {where_}",
                            id.0
                        )))
                    }
                    None => Ok(Some("this VM driver does not support watchpoints".into())),
                }
            }
            super::repl::Command::Unwatch { id } => {
                if self.driver.remove_watchpoint(crush_vm::WatchId(id)) {
                    Ok(Some(format!("watchpoint #{id} removed")))
                } else {
                    Ok(Some(format!("no watchpoint #{id}")))
                }
            }
            super::repl::Command::Print { name } => {
                // `p.x.y`: a local, then fields (or `0`-based array indices) of its value.
                let mut path = name.split('.');
                let base = path.next().unwrap_or("");
                let depth = self.driver.call_depth();
                let (slot, label) = match base.trim_start_matches('$').parse::<u16>() {
                    Ok(slot) => (slot, format!("slot {slot}")),
                    Err(_) => match self.slot_named("print", base, depth) {
                        Ok(slot) => (slot, base.to_string()),
                        Err(msg) => return Ok(Some(msg)),
                    },
                };
                let Some(mut value) = self.driver.local(depth, slot) else {
                    return Ok(Some(format!("{label} is unset in frame {depth}")));
                };
                let mut shown = label;
                for field in path {
                    match member(&value, field) {
                        Some(v) => value = v,
                        None => {
                            return Ok(Some(format!(
                                "{shown} has no field `{field}` ({})",
                                value.type_name()
                            )));
                        }
                    }
                    shown = format!("{shown}.{field}");
                }
                Ok(Some(format!("{shown} = {}", self.redactor.view(&value))))
            }
            super::repl::Command::Status => {
                let state = self.driver.state();
                let mut lines = vec![format!("instructions: {}", state.instruction_count)];
                match &state.paused_at {
                    Some(loc) => {
                        lines.push(format!("paused at: {}:{}", loc.file.display(), loc.line))
                    }
                    None => lines.push("paused at: (none)".to_string()),
                }
                Ok(Some(lines.join("\n")))
            }
            super::repl::Command::Quit => {
                unreachable!("Quit handled in run_repl before handle_command")
            }
        }
    }
}

impl<'a, D: VmDriver> DebugSession<'a, D> {
    /// The slot of local `name` in the function running at `depth`, or a
    /// message saying why it can't be resolved.
    fn slot_named(&self, verb: &str, name: &str, depth: usize) -> Result<u16, String> {
        if self.local_names.is_empty() {
            return Err(format!(
                "`{name}`: this program has no local names; use `{verb} <slot>`"
            ));
        }
        let frames = self.driver.frames(&self.redactor);
        let Some(frame) = frames.iter().find(|f| f.depth == depth) else {
            return Err(format!("`{name}`: no frame at depth {depth}"));
        };
        let function = frame.function.clone().unwrap_or_default();
        let known = self.local_names.get(&function);
        match known.and_then(|n| n.iter().find(|(n, _)| n == name)) {
            Some(&(_, slot)) => Ok(slot),
            None => {
                let mut names: Vec<&str> = known
                    .map(|n| n.iter().map(|(n, _)| n.as_str()).collect())
                    .unwrap_or_default();
                names.sort_unstable();
                Err(format!(
                    "no local `{name}` in `{function}`; its locals: {}",
                    if names.is_empty() {
                        "(none)".to_string()
                    } else {
                        names.join(", ")
                    }
                ))
            }
        }
    }

    /// ` at file:line in function` for where the program is now, or empty
    /// when the host gave no source name or the line is unknown.
    fn where_suffix(&self) -> String {
        let Some(file) = &self.source_name else {
            return String::new();
        };
        let Some(top) = self.driver.frames(&self.redactor).into_iter().next() else {
            return String::new();
        };
        let Some(line) = self.line_for_ip(top.ip) else {
            return String::new();
        };
        let text = match self.source_lines.get(line.wrapping_sub(1)) {
            Some(src) => format!("\n  {line:>4} | {src}"),
            None => String::new(),
        };
        match top.function {
            Some(f) => format!(" at {file}:{line} in {f}{text}"),
            None => format!(" at {file}:{line}{text}"),
        }
    }

    fn stepping_by_line(&self) -> bool {
        self.line_stepping && !self.source_map.is_empty()
    }

    /// The source line the program is on, from the innermost frame.
    fn current_line(&self) -> Option<usize> {
        let top = self.driver.frames(&self.redactor).into_iter().next()?;
        self.line_for_ip(top.ip)
    }

    /// `step`/`next` by source line: step by instruction until the line (or
    /// the call depth) changes. Anything else that stops the program — a
    /// breakpoint, a watchpoint, the end — ends it at once.
    fn run_line_step(&mut self, mode: crush_vm::StepMode) -> anyhow::Result<Option<String>> {
        let start_line = self.current_line();
        let start_depth = self.driver.call_depth();
        loop {
            match self.driver.step_mode(mode) {
                Ok(VmRunResult::Stepped { ip, depth }) => {
                    let moved = depth != start_depth || self.line_for_ip(ip) != start_line;
                    if moved {
                        return self.report(Ok(VmRunResult::Stepped { ip, depth }));
                    }
                }
                other => return self.report(other),
            }
        }
    }

    /// `next` / `finish`: run the step and report where it stopped.
    fn run_step(&mut self, mode: crush_vm::StepMode) -> anyhow::Result<Option<String>> {
        let result = self.driver.step_mode(mode);
        self.report(result)
    }

    /// Program output since the last command: emitted as an event and
    /// returned for the REPL message.
    fn drain_output(&mut self) -> String {
        let output = self.driver.take_output();
        if !output.is_empty() {
            self.emit(DebugEvent::Output(output.clone()));
        }
        output
    }

    fn emit_stop(&mut self, reason: StopReason) {
        if self.sink.is_some() {
            let frames = self.driver.frames(&self.redactor);
            self.emit(DebugEvent::Stopped { reason, frames });
        }
    }

    /// Turn a run's result into events and a message. Values pass through
    /// the redactor; nothing raw leaves the session.
    fn report(
        &mut self,
        result: Result<VmRunResult, super::vm_driver::VmError>,
    ) -> anyhow::Result<Option<String>> {
        let output = self.drain_output();
        let result = match result {
            Ok(r) => r,
            Err(e) => {
                self.emit(DebugEvent::Error(e.to_string()));
                return Err(anyhow::anyhow!("{}", e));
            }
        };
        let message = match result {
            VmRunResult::Done => {
                self.emit(DebugEvent::Finished);
                "done".to_string()
            }
            VmRunResult::QuotaExceeded(n) => {
                self.emit(DebugEvent::QuotaExceeded(n));
                format!("quota exceeded ({n})")
            }
            VmRunResult::HitBreakpoint(id) => {
                self.emit_stop(StopReason::Breakpoint(id));
                format!("hit breakpoint #{}{}", id.0, self.where_suffix())
            }
            VmRunResult::Paused => {
                self.emit_stop(StopReason::Paused);
                format!("paused{}", self.where_suffix())
            }
            VmRunResult::Stepped { ip, depth } => {
                self.emit_stop(StopReason::Step);
                format!("stopped at ip {ip} (depth {depth}){}", self.where_suffix())
            }
            VmRunResult::Watchpoint {
                id,
                slot,
                depth,
                old,
                new,
            } => {
                let old = old.as_ref().map(|v| self.redactor.view(v));
                let new = self.redactor.view(&new);
                let text = format!(
                    "watchpoint #{}: slot {slot} (depth {depth}) {} -> {new}{}",
                    id.0,
                    old.as_ref()
                        .map_or_else(|| "(unset)".to_string(), ToString::to_string),
                    self.where_suffix(),
                );
                self.emit_stop(StopReason::Watchpoint {
                    id,
                    slot,
                    depth,
                    old,
                    new,
                });
                text
            }
        };
        Ok(Some(format!("{output}{message}")))
    }
}

#[cfg(test)]
struct MockVmDriver {
    breakpoints: Option<BreakpointSet>,
    step_count: u64,
    visibility: crush_vm::DebugVisibility,
}

#[cfg(test)]
impl VmDriver for MockVmDriver {
    fn step(&mut self) -> Result<super::vm_driver::StepOutcome, super::vm_driver::VmError> {
        self.step_count += 1;
        Ok(super::vm_driver::StepOutcome {
            yielded: false,
            instruction_count: self.step_count,
        })
    }
    fn run_until_breakpoint_or_done(
        &mut self,
    ) -> Result<super::vm_driver::VmRunResult, super::vm_driver::VmError> {
        Ok(super::vm_driver::VmRunResult::Done)
    }
    fn set_breakpoints(&mut self, bps: &BreakpointSet) {
        self.breakpoints = Some(bps.clone());
    }
    fn state(&self) -> super::vm_driver::VmState {
        super::vm_driver::VmState {
            instruction_count: self.step_count,
            paused_at: None,
        }
    }
    fn debug_visibility(&self) -> crush_vm::DebugVisibility {
        self.visibility
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::Command;
    use std::path::PathBuf;

    fn session() -> DebugSession<'static, MockVmDriver> {
        DebugSession::<'static, _>::new(
            MockVmDriver {
                breakpoints: None,
                step_count: 0,
                visibility: crush_vm::DebugVisibility::Full,
            },
            Vec::new(),
        )
    }

    /// Returns a session with a sourcemap matching a three-line
    /// hello-world program: line 2 → offset 0 (PUSH_STR), line 3 →
    /// offset 3 (CAP_CALL), line 4 → offset 7 (HALT).
    fn session_with_sourcemap() -> DebugSession<'static, MockVmDriver> {
        DebugSession::<'static, _>::new(
            MockVmDriver {
                breakpoints: None,
                step_count: 0,
                visibility: crush_vm::DebugVisibility::Full,
            },
            vec![(2, 0), (3, 3), (4, 7)],
        )
    }

    #[test]
    fn add_breakpoint_forwards_to_driver_and_updates_session_count() {
        let mut s = session();
        let id_a = s.add_breakpoint("main.crush", 7);
        let id_b = s.add_breakpoint("main.crush", 12);
        assert_eq!(s.breakpoint_count(), 2);
        assert!(
            s.breakpoints()
                .matches(std::path::Path::new("main.crush"), 7)
        );
        assert!(
            s.breakpoints()
                .matches(std::path::Path::new("main.crush"), 12)
        );
        assert_eq!(id_a.0, 0);
        assert_eq!(id_b.0, 1);
    }

    #[test]
    fn break_on_a_line_without_code_says_which_line_has_some() {
        let mut s = session_with_sourcemap();
        let out = s
            .handle_command(Command::Break {
                file: PathBuf::from("a.crush"),
                line: 1,
            })
            .unwrap()
            .unwrap();
        assert!(out.contains("will not fire"), "{out}");
        assert!(out.contains("next line with code is 2"), "{out}");
        // A line with code carries no warning.
        let out = s
            .handle_command(Command::Break {
                file: PathBuf::from("a.crush"),
                line: 3,
            })
            .unwrap()
            .unwrap();
        assert_eq!(out, "breakpoint #1 set at a.crush:3");
    }

    #[test]
    fn a_bare_line_breaks_in_the_source_file_and_stops_name_it() {
        let mut s = session_with_sourcemap();
        s.set_source_name("prog.crush");
        let out = s
            .handle_command(crate::repl::parse_command("break 3").unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(out, "breakpoint #0 set at prog.crush:3");
        assert_eq!(s.line_for_ip(5), Some(3));
        assert_eq!(s.line_for_ip(7), Some(4));
    }

    #[test]
    fn remove_breakpoint_returns_true_for_existing() {
        let mut s = session();
        let id = s.add_breakpoint("a.crush", 1);
        assert!(s.remove_breakpoint(id));
        assert_eq!(s.breakpoint_count(), 0);
    }

    #[test]
    fn remove_breakpoint_returns_false_for_missing() {
        let mut s = session();
        let id = s.add_breakpoint("a.crush", 1);
        s.remove_breakpoint(id);
        assert!(!s.remove_breakpoint(id));
    }

    // ── handle_command tests (replaces the old todo!() panic contract) ──

    #[test]
    fn handle_command_help_returns_banner() {
        let mut s = session();
        let out = s.handle_command(Command::Help).unwrap();
        assert!(out.unwrap().contains("Commands:"));
    }

    #[test]
    fn handle_command_step_increments_and_reports() {
        let mut s = session();
        let out = s.handle_command(Command::Step).unwrap().unwrap();
        assert_eq!(out, "step 1: yielded=false");
        // Second step increments further
        let out = s.handle_command(Command::Step).unwrap().unwrap();
        assert_eq!(out, "step 2: yielded=false");
    }

    #[test]
    fn handle_command_continue_reports_done() {
        let mut s = session();
        let out = s.handle_command(Command::Continue).unwrap().unwrap();
        assert_eq!(out, "done");
    }

    #[test]
    fn handle_command_list_empty() {
        let mut s = session();
        let out = s.handle_command(Command::List).unwrap().unwrap();
        assert_eq!(out, "no breakpoints");
    }

    #[test]
    fn handle_command_list_with_breakpoints() {
        let mut s = session();
        s.add_breakpoint("a.crush", 1);
        s.add_breakpoint("b.crush", 7);
        let out = s.handle_command(Command::List).unwrap().unwrap();
        assert!(out.contains("#0: a.crush:1"));
        assert!(out.contains("#1: b.crush:7"));
    }

    #[test]
    fn handle_command_break_sets_and_reports() {
        let mut s = session();
        let out = s
            .handle_command(Command::Break {
                file: PathBuf::from("main.crush"),
                line: 42,
            })
            .unwrap()
            .unwrap();
        assert_eq!(out, "breakpoint #0 set at main.crush:42");
        assert_eq!(s.breakpoint_count(), 1);
    }

    #[test]
    fn handle_command_delete_existing() {
        let mut s = session();
        s.add_breakpoint("a.crush", 1);
        let out = s
            .handle_command(Command::Delete { id: 0 })
            .unwrap()
            .unwrap();
        assert_eq!(out, "breakpoint #0 removed");
        assert_eq!(s.breakpoint_count(), 0);
    }

    #[test]
    fn handle_command_delete_missing() {
        let mut s = session();
        let out = s
            .handle_command(Command::Delete { id: 99 })
            .unwrap()
            .unwrap();
        assert_eq!(out, "no breakpoint #99");
    }

    #[test]
    fn handle_command_print_by_name_explains_slots() {
        let mut s = session();
        let out = s
            .handle_command(Command::Print {
                name: "x".to_string(),
            })
            .unwrap()
            .unwrap();
        assert!(out.contains("use `print <slot>`"), "{out}");
    }

    /// A driver without step/watch support says so instead of failing
    /// silently (the mock keeps the trait's defaults).
    #[test]
    fn handle_command_next_and_watch_on_a_driver_without_support() {
        let mut s = session();
        let err = s.handle_command(Command::Next).unwrap_err();
        assert!(err.to_string().contains("does not support step"), "{err}");
        let out = s
            .handle_command(Command::Watch {
                slot: 0,
                scope: None,
            })
            .unwrap()
            .unwrap();
        assert!(out.contains("does not support watchpoints"), "{out}");
    }

    /// `status` should report instruction count and no pause point.
    #[test]
    fn handle_command_status_reports_instructions_and_no_pause() {
        let mut s = session();
        // Step once so instruction_count is non-zero
        let _ = s.handle_command(Command::Step).unwrap();
        let out = s.handle_command(Command::Status).unwrap().unwrap();
        assert!(out.contains("instructions: 1"), "status output: {out}");
        assert!(out.contains("paused at: (none)"), "status output: {out}");
    }

    /// `status` should show `paused at: (none)` when the VM is idle
    /// with no breakpoints hit yet.
    #[test]
    fn handle_command_status_reports_no_pause_when_idle() {
        let mut s = session();
        let out = s.handle_command(Command::Status).unwrap().unwrap();
        assert!(out.contains("instructions: 0"));
        assert!(out.contains("paused at: (none)"));
    }

    /// Panic-pin: Quit should never reach handle_command (run_repl
    /// intercepts it). If a refactor accidentally routes Quit here,
    /// this test catches it.
    #[test]
    #[should_panic(expected = "Quit handled in run_repl")]
    fn handle_command_quit_panics() {
        let mut s = session();
        let _ = s.handle_command(Command::Quit);
    }

    // ── CRUSH-160: grant gating ────────────────────────────────────

    fn session_at(level: crush_vm::DebugVisibility) -> DebugSession<'static, MockVmDriver> {
        DebugSession::<'static, _>::new(
            MockVmDriver {
                breakpoints: None,
                step_count: 0,
                visibility: level,
            },
            Vec::new(),
        )
    }

    fn control_commands() -> Vec<Command> {
        vec![
            Command::Break {
                file: PathBuf::from("a.crush"),
                line: 1,
            },
            Command::Delete { id: 0 },
            Command::Step,
            Command::Next,
            Command::Finish,
            Command::Continue,
            Command::Watch {
                slot: 0,
                scope: None,
            },
            Command::Unwatch { id: 0 },
        ]
    }

    /// No `debug.step`: every control command is refused before it reaches
    /// the VM, with a `Refused` event naming the grant.
    #[test]
    fn without_debug_step_every_control_command_is_refused() {
        let mut s = session_at(crush_vm::DebugVisibility::None);
        let sink = crate::events::CollectingSink::new();
        s.set_event_sink(sink.clone());
        for cmd in control_commands() {
            let out = s.handle_command(cmd.clone()).unwrap().unwrap();
            assert!(
                out.starts_with("refused:") && out.contains("`debug.step`"),
                "{cmd:?}: {out}"
            );
        }
        assert_eq!(s.driver.step_count, 0, "a refused command reached the VM");
        assert_eq!(s.breakpoint_count(), 0);
        let refused = sink
            .events()
            .into_iter()
            .filter(|e| {
                matches!(
                    e,
                    DebugEvent::Refused {
                        needs: "debug.step",
                        ..
                    }
                )
            })
            .count();
        assert_eq!(refused, control_commands().len());
        // Read-only commands still answer.
        assert!(s.handle_command(Command::Help).unwrap().is_some());
        assert_eq!(
            s.handle_command(Command::List).unwrap().unwrap(),
            "no breakpoints"
        );
    }

    #[test]
    fn control_only_steps_but_refuses_print() {
        let mut s = session_at(crush_vm::DebugVisibility::ControlOnly);
        assert_eq!(
            s.handle_command(Command::Step).unwrap().unwrap(),
            "step 1: yielded=false"
        );
        let out = s
            .handle_command(Command::Print { name: "0".into() })
            .unwrap()
            .unwrap();
        assert!(
            out.contains("refused") && out.contains("debug.inspect.redacted"),
            "{out}"
        );
    }

    #[test]
    fn inspect_levels_allow_print() {
        for level in [
            crush_vm::DebugVisibility::InspectRedacted,
            crush_vm::DebugVisibility::Full,
        ] {
            let mut s = session_at(level);
            let out = s
                .handle_command(Command::Print { name: "0".into() })
                .unwrap()
                .unwrap();
            assert!(!out.contains("refused"), "{level:?}: {out}");
        }
    }

    // ── sourcemap resolution tests ─────────────────────────────────

    /// When the sourcemap contains a matching line, `add_breakpoint`
    /// resolves the bytecode address and does NOT emit a warning.
    /// This is the regression pin for the false-positive scenario
    /// where the sourcemap wasn't extracted before the program was
    /// moved into PortableVm.
    #[test]
    fn add_breakpoint_resolves_line_to_bytecode_address_via_sourcemap() {
        let mut s = session_with_sourcemap();
        let id = s.add_breakpoint("hello.crush", 2);
        assert_eq!(id.0, 0);
        assert_eq!(s.breakpoint_count(), 1);
        // The breakpoint at line 2 should have bytecode_address = Some(0)
        let bp = &s.breakpoints().list()[0];
        assert_eq!(
            bp.bytecode_address,
            Some(0),
            "line 2 should resolve to bytecode offset 0 via sourcemap"
        );
    }

    /// When the sourcemap is non-empty but the line isn't found,
    /// the breakpoint's `bytecode_address` stays `None` (the warning
    /// on stderr is a side effect checked by integration tests).
    #[test]
    fn add_breakpoint_leaves_address_none_when_line_not_in_sourcemap() {
        let mut s = session_with_sourcemap();
        let id = s.add_breakpoint("hello.crush", 99);
        assert_eq!(id.0, 0);
        let bp = &s.breakpoints().list()[0];
        assert_eq!(
            bp.bytecode_address, None,
            "line 99 should NOT resolve (not in sourcemap)"
        );
    }
}
