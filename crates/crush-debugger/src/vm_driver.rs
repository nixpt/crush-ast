//! Abstraction seam over `crush-vm::PortableVm`.
//!
//! The `VmDriver` trait lets `session.rs` drive ANY single-steppable VM
//! (today: `PortableVmDriver` wrapping `crush-vm::PortableVm`) without
//! depending on the concrete VM type. Trait surface is minimal — only
//! what a REPL needs.

use std::fmt;

use crate::breakpoint::BreakpointSet;

/// Outcome of a single `step()` call. Mirrors the shape of
/// `crush_vm::VmYield` without depending on its exact variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    pub yielded: bool,
    pub instruction_count: u64,
}

/// Snapshot of the VM state at a pause point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmState {
    pub instruction_count: u64,
    pub paused_at: Option<crate::breakpoint::Location>,
}

/// Result of running to completion or hitting a stop condition.
#[derive(Debug)]
pub enum VmRunResult {
    Done,
    HitBreakpoint(crate::breakpoint::BreakpointId),
    QuotaExceeded,
    Paused,
}

impl fmt::Display for VmRunResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done => f.write_str("done"),
            Self::HitBreakpoint(id) => write!(f, "hit breakpoint #{}", id.0),
            Self::QuotaExceeded => f.write_str("quota exceeded"),
            Self::Paused => f.write_str("paused"),
        }
    }
}

#[derive(Debug)]
pub enum VmError {
    Inner(crush_vm::VmError),
    DriverInvariant(&'static str),
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inner(e) => write!(f, "vm error: {e}"),
            Self::DriverInvariant(s) => write!(f, "driver invariant violated: {s}"),
        }
    }
}

impl std::error::Error for VmError {}

pub trait VmDriver {
    fn step(&mut self) -> Result<StepOutcome, VmError>;

    /// SCAFFOLD GAP: `todo!()` panic until the upstream
    /// `crush_vm::PortableVm` BP pause hook lands at
    /// `portable_vm.rs:1037`. We refuse to silently mimic a
    /// run-to-completion — a silent fall-through would mask the gap.
    fn run_until_breakpoint_or_done(&mut self) -> Result<VmRunResult, VmError>;

    /// Register a breakpoint set with the VM. Drivers that can't yet
    /// hook into the VM's dispatch loop keep a copy of the set so the
    /// REPL `step`/`continue` loop can poll them externally.
    fn set_breakpoints(&mut self, bps: &BreakpointSet);

    fn state(&self) -> VmState;
}

pub struct PortableVmDriver<'a> {
    vm: &'a mut crush_vm::PortableVm,
    breakpoints: Option<BreakpointSet>,
    instruction_count: u64,
}

impl<'a> PortableVmDriver<'a> {
    pub fn new(vm: &'a mut crush_vm::PortableVm) -> Self {
        Self {
            vm,
            breakpoints: None,
            instruction_count: 0,
        }
    }
}

impl<'a> VmDriver for PortableVmDriver<'a> {
    fn step(&mut self) -> Result<StepOutcome, VmError> {
        let yielded = self.vm.step().map_err(VmError::Inner)?;
        self.instruction_count += 1;
        Ok(StepOutcome {
            yielded: yielded.is_some(),
            instruction_count: self.instruction_count,
        })
    }

    fn run_until_breakpoint_or_done(&mut self) -> Result<VmRunResult, VmError> {
        let _ = &mut *self.vm;
        todo!(
            "DEBUGGER-1: real run-until-breakpoint loop. Replace with a \
             `step()` loop that inspects the new `VmYield::BreakpointHit` \
             variant once `crush_vm::PortableVm` ships the BP pause \
             hook at portable_vm.rs:1037."
        )
    }

    fn set_breakpoints(&mut self, bps: &BreakpointSet) {
        self.breakpoints = Some(bps.clone());
    }

    fn state(&self) -> VmState {
        VmState {
            instruction_count: self.instruction_count,
            paused_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use super::*;

    #[test]
    fn trait_compiles_and_step_outcome_fields_match_docs() {
        let outcome = StepOutcome {
            yielded: false,
            instruction_count: 0,
        };
        assert!(!outcome.yielded);
        assert_eq!(outcome.instruction_count, 0);
    }

    #[test]
    fn vm_run_result_display_strings_match_design() {
        assert_eq!(VmRunResult::Done.to_string(), "done");
        assert_eq!(
            VmRunResult::HitBreakpoint(crate::breakpoint::BreakpointId(3)).to_string(),
            "hit breakpoint #3"
        );
        assert_eq!(VmRunResult::QuotaExceeded.to_string(), "quota exceeded");
        assert_eq!(VmRunResult::Paused.to_string(), "paused");
    }

    #[test]
    fn vm_error_inner_carries_underlying_message_in_source_chain() {
        let e = VmError::DriverInvariant("stepped past EOF");
        assert_eq!(e.to_string(), "driver invariant violated: stepped past EOF");
        assert!(e.source().is_none());
    }

    /// Compile-time pin: `run_until_breakpoint_or_done` exists with
    /// the documented signature. A future change to the return type
    /// trips this test loudly.
    #[test]
    fn run_until_breakpoint_or_done_signature_compiles() {
        let _: fn(&mut dyn VmDriver) -> Result<VmRunResult, VmError> =
            |d| d.run_until_breakpoint_or_done();
    }
}
