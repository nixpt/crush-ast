//! crush-debugger: interactive runtime debugger for Crush packages.
//!
//! # Status: SCAFFOLD (initial commit)
//!
//! This crate ships five composable modules that together form the
//! skeleton of a runtime debugger for Crush packages. The surface is
//! real; what is intentionally wired behind `todo!()` (with documented
//! hook points) is whatever requires a companion change upstream:
//!
//! - [`wire_consumer`]: parse the `emit_post_dispatch_lint` NDJSON
//!   `DiagRecord` stream from any source (stdin, subprocess, file) into
//!   an owned, round-trip-tested record shape.
//! - [`breakpoint`]: a breakpoint registry keyed by `<file>:<line>`,
//!   URL-fragment-aware thanks to the upstream `scan_entry_file_references`
//!   fix (see agent/buffy/network @ 2f2b2f5).
//! - [`repl`]: command parser for the interactive REPL
//!   (`break`, `step`, `continue`, `print`, `list`, `quit`, `help`).
//! - [`vm_driver`]: the abstraction seam (`VmDriver` trait) over
//!   `crush-vm::PortableVm` so REPL + session don't bind to a concrete VM.
//! - [`session`]: owns the debugger session lifecycle (target capsule,
//!   attached driver, breakpoint registry, REPL invocation). The REPL
//!   eval loop is wired end-to-end; breakpoint pause is hooked into
//!   `crush_vm::PortableVm::step()` via `VmYield::DebugBreak`.
//!
//! # Hook points that deliberately use `todo!()`
//!
//! 1. **Source `file:line` -> bytecode address.** A breakpoint request
//!    is stored by source location; the bytecode-coord mapping will land
//!    alongside an upcoming `crush-frontend` sourcemap. Until then,
//!    only breakpoints with `bytecode_address` set (cast.json or manual)
//!    will trigger in the VM.
//! 2. **Programmatic breakpoint at bytecode offset.** The VM hook in
//!    `portable_vm.rs` supports `set_breakpoints(&[usize])` for bytecode-
//!    level pause; a future `crush-frontend` sourcemap will close the
//!    `file:line -> offset` gap.

pub mod breakpoint;
pub mod repl;
pub mod session;
pub mod vm_driver;
pub mod wire_consumer;

pub use breakpoint::{BreakpointId, BreakpointSet, Location};
pub use repl::{Command, ParseCommandError, parse_breakpoint_arg, parse_command};
pub use session::DebugSession;
pub use vm_driver::{PortableVmDriver, StepOutcome, VmDriver, VmError, VmRunResult, VmState};
pub use wire_consumer::{OwnedDiagRecord, ParseRecordError, consume_stream, parse_record};
