//! crush-debugger: interactive runtime debugger for Crush packages.
//!
//! Drives `crush_vm::PortableVm` through a [`VmDriver`] and pauses on the
//! VM's own hooks: every stop is a `VmYield::DebugBreak`, with the reason
//! in `PortableVm::last_stop()`.
//!
//! - [`breakpoint`]: breakpoints keyed by `<file>:<line>`, resolved to a
//!   bytecode offset through the assembler's source map.
//! - [`repl`]: the command parser (`break`, `step`, `next`, `finish`,
//!   `continue`, `watch`, `unwatch`, `print`, `list`, `status`, `quit`).
//! - [`vm_driver`]: the `VmDriver` seam over `PortableVm`, so the session
//!   doesn't bind to a concrete VM.
//! - [`session`]: the session lifecycle and the REPL loop.
//! - NDJSON diagnostics: [`OwnedDiagRecord`] / [`parse_record`] /
//!   [`consume_stream`], re-exported from `crush_diagnostics::wire_consumer`.
//!
//! - [`events`]: [`DebugEvent`]s for embedding hosts, through a
//!   [`DebugEventSink`] (a channel sender, or [`CollectingSink`]).
//!
//! Debugging is grant-gated (CRUSH-160): the session reads its
//! `DebugVisibility` from the VM's `debug.*` host-capability grants once,
//! at construction. Without `debug.step` every command that controls the
//! program is refused; values are hidden unless `debug.inspect.redacted`
//! (type + keyed hash) or `debug.inspect` (in full) is granted too.
//!
//! Stepping and watchpoints are bytecode-level (CRUSH-159): `step` runs one
//! instruction, `next` runs a call to completion, `finish` runs until the
//! current function returns, all by call depth; `watch <slot>` stops after
//! an instruction changes a local slot. Line-level stepping and watching by
//! variable name need a source map from `crush-frontend` (today only the
//! assembler produces one, for breakpoints in `.crush` assembly).

pub mod breakpoint;
pub mod events;
pub mod repl;
pub mod session;
pub mod vm_driver;

pub use breakpoint::{BreakpointId, BreakpointSet, Location};
pub use events::{CollectingSink, DebugEvent, DebugEventSink, StopReason};
pub use repl::{Command, ParseCommandError, parse_breakpoint_arg, parse_command};
pub use session::DebugSession;
pub use vm_driver::{PortableVmDriver, StepOutcome, VmDriver, VmError, VmRunResult, VmState};
// The NDJSON consumer (OwnedDiagRecord / parse_record / consume_stream /
// ParseRecordError) now lives canonically in `crush_diagnostics::wire_consumer`.
// Re-export from here for back-compat with existing `crush_debugger::*`
// call sites.
pub use crush_diagnostics::{
    consume_stream, parse_record, OwnedDiagRecord, ParseRecordError,
};
