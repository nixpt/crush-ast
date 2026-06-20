//! Debug session: owns the VM driver + breakpoint registry +
//! assembler sourcemap.
//!
//! The session resolves `file:line` breakpoints to bytecode offsets
//! via the sourcemap at add-time, so the VM driver can forward them
//! to `PortableVm` for native pause-on-hit.

use std::marker::PhantomData;

use super::breakpoint::{BreakpointId, BreakpointSet};
use super::vm_driver::VmDriver;

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

/// REPL help text shown on `help` / `h` / `?`.
const HELP_BANNER: &str = "\
Commands:
  break <file>:<line>  — set a breakpoint
  delete <id>          — remove a breakpoint
  step | s             — single-step the VM
  continue | c         — run until breakpoint or done
  list | l             — list all breakpoints
  status | info | i    — show VM state
  print <var> | p      — print variable value (NYI)
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
    /// Tie the session's lifetime to the VM's; without it the `'a`
    /// would be unused and Rust would reject the struct.
    _vm_ref: PhantomData<&'a ()>,
}

impl<'a, D: VmDriver> DebugSession<'a, D> {
    /// Construct a session around an already-attached VM driver.
    /// `source_map` is the assembler line→offset mapping; pass
    /// `Vec::new()` for programs without debug info.
    pub fn new(driver: D, source_map: Vec<(usize, usize)>) -> Self {
        Self {
            driver,
            breakpoints: BreakpointSet::new(),
            source_map,
            _vm_ref: PhantomData,
        }
    }

    /// Register a source-level breakpoint and forward the updated set
    /// to the underlying driver. Resolves `file:line` to a bytecode
    /// offset via the assembler sourcemap if available.
    pub fn add_breakpoint(
        &mut self,
        file: impl Into<std::path::PathBuf>,
        line: u32,
    ) -> BreakpointId {
        let file = file.into();
        let addr = self
            .source_map
            .iter()
            .find(|(l, _)| *l == line as usize)
            .map(|(_, offset)| *offset as u32);
        if addr.is_none() && !self.source_map.is_empty() {
            eprintln!(
                "warning: breakpoint at {}:{} has no matching bytecode address \
                 (line not in sourcemap)",
                file.display(),
                line
            );
        }
        let id = self.breakpoints.add(file, line, addr);
        self.driver.set_breakpoints(&self.breakpoints);
        id
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
    /// The heuristic step-loop in `vm_driver.rs` polyfills the missing
    /// upstream BP pause hook — breakpoints are registered but cannot
    /// be checked mid-step until the sourcemap lands.
    pub fn run_repl(&mut self) -> anyhow::Result<()> {
        use std::io::{self, BufRead, Write};
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut reader = stdin.lock();
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
    /// `pub(crate)` so the test module can exercise it directly without
    /// mocking stdin.
    pub(crate) fn handle_command(
        &mut self,
        cmd: super::repl::Command,
    ) -> anyhow::Result<Option<String>> {
        match cmd {
            super::repl::Command::Help => Ok(Some(HELP_BANNER.to_string())),
            super::repl::Command::Step => match self.driver.step() {
                Ok(outcome) => Ok(Some(format!(
                    "step {}: yielded={}",
                    outcome.instruction_count, outcome.yielded
                ))),
                Err(super::vm_driver::VmError::Inner(ref e)) if quota_n(e).is_some() => {
                    Ok(Some(quota_message(e)))
                }
                Err(e) => Err(anyhow::anyhow!("{}", e)),
            },
            super::repl::Command::Continue => {
                let result = self
                    .driver
                    .run_until_breakpoint_or_done()
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                Ok(Some(result.to_string()))
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
            super::repl::Command::Break { file, line } => {
                let id = self.add_breakpoint(file.clone(), line);
                Ok(Some(format!(
                    "breakpoint #{} set at {}:{}",
                    id.0,
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
            super::repl::Command::Print { name } => {
                Ok(Some(format!("<print {}: not yet implemented>", name)))
            }
            super::repl::Command::Status => {
                let state = self.driver.state();
                let mut lines = vec![format!(
                    "instructions: {}",
                    state.instruction_count
                )];
                match &state.paused_at {
                    Some(loc) => lines.push(format!(
                        "paused at: {}:{}",
                        loc.file.display(),
                        loc.line
                    )),
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

#[cfg(test)]
struct MockVmDriver {
    breakpoints: Option<BreakpointSet>,
    step_count: u64,
}

#[cfg(test)]
impl VmDriver for MockVmDriver {
    fn step(
        &mut self,
    ) -> Result<super::vm_driver::StepOutcome, super::vm_driver::VmError> {
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
        assert!(s.breakpoints().matches(std::path::Path::new("main.crush"), 7));
        assert!(s.breakpoints().matches(std::path::Path::new("main.crush"), 12));
        assert_eq!(id_a.0, 0);
        assert_eq!(id_b.0, 1);
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
        let out = s
            .handle_command(Command::Continue)
            .unwrap()
            .unwrap();
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
    fn handle_command_print_not_yet_implemented() {
        let mut s = session();
        let out = s
            .handle_command(Command::Print {
                name: "x".to_string(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(out, "<print x: not yet implemented>");
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
            bp.bytecode_address,
            None,
            "line 99 should NOT resolve (not in sourcemap)"
        );
    }
}
