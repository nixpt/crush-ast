//! Abstraction seam over `crush-vm::PortableVm`.
//!
//! The `VmDriver` trait lets `session.rs` drive ANY single-steppable VM
//! (today: `PortableVmDriver` wrapping `crush-vm::PortableVm`) without
//! depending on the concrete VM type. Trait surface is minimal — only
//! what a REPL needs. Methods added after the first release (step modes,
//! watchpoints, locals) have defaults, so existing drivers keep compiling.

use std::collections::HashMap;
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
    QuotaExceeded(usize),
    Paused,
    /// A `next`/`finish`/`step_mode` request finished; the VM is paused
    /// before the instruction at `ip`, `depth` frames deep.
    Stepped {
        ip: usize,
        depth: usize,
    },
    /// The last instruction changed a watched slot (it has already run).
    Watchpoint {
        id: crush_vm::WatchId,
        slot: u16,
        depth: usize,
        old: Option<crush_vm::vm::Value>,
        new: crush_vm::vm::Value,
    },
}

/// Text for a value in debugger output: strings quoted, everything else as
/// the VM prints it.
pub fn show_value(v: &crush_vm::vm::Value) -> String {
    match v {
        crush_vm::vm::Value::Str(s) => format!("{s:?}"),
        other => crush_vm::value_to_text(other),
    }
}

impl fmt::Display for VmRunResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Done => f.write_str("done"),
            Self::HitBreakpoint(id) => write!(f, "hit breakpoint #{}", id.0),
            Self::QuotaExceeded(n) => write!(f, "quota exceeded ({n})"),
            Self::Paused => f.write_str("paused"),
            Self::Stepped { ip, depth } => write!(f, "stopped at ip {ip} (depth {depth})"),
            Self::Watchpoint {
                id,
                slot,
                depth,
                old,
                new,
            } => write!(
                f,
                "watchpoint #{}: slot {slot} (depth {depth}) {} -> {}",
                id.0,
                old.as_ref()
                    .map_or_else(|| "(unset)".to_string(), show_value),
                show_value(new)
            ),
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

    /// Run to the next breakpoint or termination. Uses the VM's
    /// native breakpoint hook (`VmYield::DebugBreak`) to pause at
    /// registered bytecode addresses.
    fn run_until_breakpoint_or_done(&mut self) -> Result<VmRunResult, VmError>;

    /// Register a breakpoint set with the VM. Forwarded to the
    /// underlying VM for bytecode-level pause-on-hit.
    fn set_breakpoints(&mut self, bps: &BreakpointSet);

    fn state(&self) -> VmState;

    /// Run a step of the given kind (into/over/out by call depth) and
    /// report where it stopped. A breakpoint or watchpoint on the way ends
    /// it early. Drivers without step support return `DriverInvariant`.
    fn step_mode(&mut self, _mode: crush_vm::StepMode) -> Result<VmRunResult, VmError> {
        Err(VmError::DriverInvariant(
            "this driver does not support step over/out",
        ))
    }

    /// Watch local `slot` in `scope`; `None` if the driver can't watch.
    fn add_watchpoint(
        &mut self,
        _slot: u16,
        _scope: crush_vm::WatchScope,
    ) -> Option<crush_vm::WatchId> {
        None
    }

    /// Remove a watchpoint. Returns whether it existed.
    fn remove_watchpoint(&mut self, _id: crush_vm::WatchId) -> bool {
        false
    }

    /// Current call depth (1 = entry frame; 0 if unknown).
    fn call_depth(&self) -> usize {
        0
    }

    /// Local `slot` of the frame at `depth`, if assigned.
    fn local(&self, _depth: usize, _slot: u16) -> Option<crush_vm::vm::Value> {
        None
    }

    /// What a debug client may do, from the VM's `debug.*` grants. The
    /// default grants nothing: a driver must opt in.
    fn debug_visibility(&self) -> crush_vm::DebugVisibility {
        crush_vm::DebugVisibility::None
    }

    /// Every frame, innermost first, rendered by `redactor`.
    fn frames(&self, _redactor: &crush_vm::Redactor) -> Vec<crush_vm::FrameSnapshot> {
        Vec::new()
    }

    /// Program output printed since the last call.
    fn take_output(&mut self) -> String {
        String::new()
    }
}

impl<'a> PortableVmDriver<'a> {
    /// The registered breakpoint at the VM's current IP. `hit_index`
    /// selects which matching breakpoint to return (0 = first, 1 =
    /// second, …). Used when multiple breakpoints share the same
    /// bytecode address.
    fn bp_at_current_ip(
        &self,
        hit_index: usize,
    ) -> Option<&crate::breakpoint::Breakpoint> {
        let ip = self.vm.current_ip();
        self.breakpoints
            .as_ref()?
            .list()
            .into_iter()
            .filter(|bp| bp.bytecode_address == Some(ip as u32))
            .nth(hit_index)
    }
}

pub struct PortableVmDriver<'a> {
    vm: &'a mut crush_vm::PortableVm,
    breakpoints: Option<BreakpointSet>,
    instruction_count: u64,
    /// Per-IP counter: how many breakpoints at each bytecode
    /// address have already fired. Mirrors the VM's internal
    /// `breakpoint_hit` counters so the driver can report the
    /// correct breakpoint ID when multiple BPs share an address.
    breakpoint_hit: HashMap<usize, usize>,
    /// Per-IP total: how many breakpoints are registered at each
    /// bytecode offset. Precomputed in `set_breakpoints()` so the
    /// hot path in `run_until_breakpoint_or_done` is an O(1)
    /// lookup instead of O(n) `.filter().count()`.
    breakpoint_count: HashMap<usize, usize>,
    /// The source-level location of the last breakpoint hit.
    /// Persists across counter cleanup so `state()` reports the
    /// correct pause point. Cleared when execution resumes.
    paused_bp: Option<crate::breakpoint::Location>,
}

impl<'a> PortableVmDriver<'a> {
    pub fn new(vm: &'a mut crush_vm::PortableVm) -> Self {
        Self {
            vm,
            breakpoints: None,
            instruction_count: 0,
            breakpoint_hit: HashMap::new(),
            breakpoint_count: HashMap::new(),
            paused_bp: None,
        }
    }
}

impl<'a> VmDriver for PortableVmDriver<'a> {
    fn step(&mut self) -> Result<StepOutcome, VmError> {
        let yielded = self.vm.step().map_err(VmError::Inner)?;
        self.instruction_count += 1;
        // Keep the driver's per-IP hit counter in sync with the VM.
        // When the VM reports a DebugBreak, the VM's internal
        // counter advanced but the instruction didn't execute —
        // track it here so `run_until_breakpoint_or_done` can
        // report the correct breakpoint ID.
        let breakpoint_stop = matches!(
            self.vm.last_stop(),
            Some(crush_vm::DebugStop::Breakpoint { .. })
        );
        if yielded.is_some() && breakpoint_stop {
            let ip = self.vm.current_ip();
            self.breakpoint_hit.entry(ip).and_modify(|c| *c += 1).or_insert(1);
            // Capture the paused-at location so `state()` reports
            // the correct breakpoint even after the hit counter is
            // cleaned up by `run_until_breakpoint_or_done`.
            let count = *self.breakpoint_hit.get(&ip).unwrap_or(&0);
            self.paused_bp = self
                .bp_at_current_ip(count.saturating_sub(1))
                .map(|bp| bp.location.clone());
        } else {
            // Instruction executed — clear the paused-at location.
            self.paused_bp = None;
        }
        Ok(StepOutcome {
            yielded: yielded.is_some(),
            instruction_count: self.instruction_count,
        })
    }

    fn run_until_breakpoint_or_done(&mut self) -> Result<VmRunResult, VmError> {
        loop {
            if self.vm.is_halted() {
                return Ok(VmRunResult::Done);
            }
            let outcome = match self.step() {
                Ok(o) => o,
                Err(VmError::Inner(crush_vm::VmError::StepQuota(n)))
                | Err(VmError::Inner(crush_vm::VmError::StackQuota(n)))
                | Err(VmError::Inner(crush_vm::VmError::OutputQuota(n)))
                | Err(VmError::Inner(crush_vm::VmError::CallDepthQuota(n))) => {
                    return Ok(VmRunResult::QuotaExceeded(n));
                }
                Err(e) => return Err(e),
            };
            if outcome.yielded {
                match self.vm.last_stop() {
                    Some(crush_vm::DebugStop::Step { ip, .. }) => {
                        return Ok(VmRunResult::Stepped {
                            ip: *ip,
                            depth: self.vm.call_depth(),
                        });
                    }
                    Some(crush_vm::DebugStop::Watch {
                        id,
                        slot,
                        depth,
                        old,
                        new,
                    }) => {
                        return Ok(VmRunResult::Watchpoint {
                            id: *id,
                            slot: *slot,
                            depth: *depth,
                            old: old.clone(),
                            new: new.clone(),
                        });
                    }
                    _ => {}
                }
                let ip = self.vm.current_ip();
                // The per-IP counter was already incremented by
                // `step()` when the VM yielded DebugBreak. Guard
                // against a non-breakpoint yield (count == 0)
                // which would cause `saturating_sub(1)` to return
                // index 0 and potentially misidentify an unrelated
                // breakpoint as having fired.
                let count = self.breakpoint_hit.get(&ip).copied().unwrap_or(0);
                if count == 0 {
                    return Ok(VmRunResult::Paused);
                }
                // Safe: the guard guarantees count >= 1.
                let idx = count - 1;
                let bp = self
                    .bp_at_current_ip(idx)
                    .map(|bp| (bp.id, bp.location.clone()));
                let (bp_id, bp_loc) = match bp {
                    Some((id, loc)) => (id, Some(loc)),
                    None => (crate::breakpoint::BreakpointId(0), None),
                };
                // Record the paused-at location so `state()` reports
                // it even after the hit counter is cleaned up.
                self.paused_bp = bp_loc;
                // When all breakpoints at this IP have fired, the
                // VM clears its own counter and executes the
                // instruction. Sync the driver's counter.
                let total = self.breakpoint_count.get(&ip).copied().unwrap_or(0);
                if count >= total {
                    self.breakpoint_hit.remove(&ip);
                }
                return Ok(VmRunResult::HitBreakpoint(bp_id));
            }
        }
    }

    /// Register a breakpoint set with the VM. Forwards bytecode
    /// addresses (those with `bytecode_address` set) to the
    /// underlying `PortableVm`. Breakpoints without a bytecode
    /// address are kept in the driver's registry but won't trigger
    /// until the sourcemap lands (DEBUGGER-2).
    ///
    /// Resets per-IP hit counters so re-registered breakpoints fire
    /// again from the first matching ID.
    fn set_breakpoints(&mut self, bps: &BreakpointSet) {
        self.breakpoints = Some(bps.clone());
        self.breakpoint_hit.clear();
        self.breakpoint_count.clear();
        self.paused_bp = None;
        // Precompute per-IP breakpoint totals.
        for bp in bps.list() {
            if let Some(a) = bp.bytecode_address {
                *self.breakpoint_count.entry(a as usize).or_insert(0) += 1;
            }
        }
        let ips: Vec<usize> = bps
            .list()
            .iter()
            .filter_map(|bp| bp.bytecode_address.map(|a| a as usize))
            .collect();
        self.vm.set_breakpoints(&ips);
    }

    fn state(&self) -> VmState {
        VmState {
            instruction_count: self.instruction_count,
            paused_at: self.paused_bp.clone(),
        }
    }

    fn step_mode(&mut self, mode: crush_vm::StepMode) -> Result<VmRunResult, VmError> {
        self.vm.request_step(mode);
        let result = self.run_until_breakpoint_or_done();
        // A quota or error ended the run: don't leave the request armed.
        self.vm.cancel_step();
        result
    }

    fn add_watchpoint(
        &mut self,
        slot: u16,
        scope: crush_vm::WatchScope,
    ) -> Option<crush_vm::WatchId> {
        Some(self.vm.add_watchpoint(slot, scope))
    }

    fn remove_watchpoint(&mut self, id: crush_vm::WatchId) -> bool {
        self.vm.remove_watchpoint(id)
    }

    fn call_depth(&self) -> usize {
        self.vm.call_depth()
    }

    fn local(&self, depth: usize, slot: u16) -> Option<crush_vm::vm::Value> {
        self.vm.local(depth, slot).cloned()
    }

    fn debug_visibility(&self) -> crush_vm::DebugVisibility {
        self.vm.debug_visibility()
    }

    fn frames(&self, redactor: &crush_vm::Redactor) -> Vec<crush_vm::FrameSnapshot> {
        (1..=self.vm.call_depth())
            .rev()
            .filter_map(|d| self.vm.frame_snapshot(d, redactor))
            .collect()
    }

    fn take_output(&mut self) -> String {
        self.vm.take_output()
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
        assert_eq!(VmRunResult::QuotaExceeded(5000).to_string(), "quota exceeded (5000)");
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
