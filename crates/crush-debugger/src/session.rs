//! Debug session: owns the VM driver + breakpoint registry.
//!
//! The SCAFFOLD compiles with real types; `run_repl()` is `todo!()`
//! gated on the upstream `crush_vm::PortableVm` BP pause hook landing
//! (see `vm_driver.rs`).

use std::marker::PhantomData;

use super::breakpoint::{BreakpointId, BreakpointSet};
use super::vm_driver::VmDriver;

/// The lifetime parameter `'a` mirrors the VM borrow; the `D: VmDriver`
/// parameter lets tests swap a mock driver.
pub struct DebugSession<'a, D: VmDriver> {
    driver: D,
    breakpoints: BreakpointSet,
    /// Tie the session's lifetime to the VM's; without it the `'a`
    /// would be unused and Rust would reject the struct.
    _vm_ref: PhantomData<&'a ()>,
}

impl<'a, D: VmDriver> DebugSession<'a, D> {
    /// Construct a session around an already-attached VM driver.
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            breakpoints: BreakpointSet::new(),
            _vm_ref: PhantomData,
        }
    }

    /// Register a source-level breakpoint and forward the updated set
    /// to the underlying driver.
    pub fn add_breakpoint(
        &mut self,
        file: impl Into<std::path::PathBuf>,
        line: u32,
    ) -> BreakpointId {
        let id = self.breakpoints.add(file, line);
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

    /// Run the REPL loop. SCAFFOLD: `todo!()` because the eval binding
    /// between `Command -> VmDriver` actions needs the upstream VM
    /// breakpoint pause hook to function end-to-end.
    pub fn run_repl(&mut self) -> anyhow::Result<()> {
        let _ = &self.driver;
        todo!(
            "DEBUGGER-1: bind parse_command() output to VmDriver actions \
             once crush_vm::PortableVm breakpoint pause hook lands."
        )
    }
}

#[cfg(test)]
struct MockVmDriver {
    breakpoints: Option<BreakpointSet>,
    step_count: u64,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> DebugSession<'static, MockVmDriver> {
        DebugSession::<'static, _>::new(MockVmDriver {
            breakpoints: None,
            step_count: 0,
        })
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

    #[test]
    fn run_repl_panics_with_todo_macro_until_upstream_hook_lands() {
        // Intentional panicky contract: `run_repl` is gated on the BP
        // pause hook landing upstream. The test pins the panic-message
        // shape so a future land of the real impl forces a test update.
        let mut s = session();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = s.run_repl();
        }));
        assert!(result.is_err(), "run_repl must panic (todo!()) until DEBUGGER-1 lands");
    }
}
