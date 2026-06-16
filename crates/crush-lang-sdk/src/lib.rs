//! # CRUSH Language SDK
//!
//! A Rust SDK for hosting and extending the standalone [`crush-vm`] CVM1
//! bytecode runtime. It provides a higher-level, ergonomic API over the raw
//! VM crate for loading, assembling, and executing CRUSH programs.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use crush_lang_sdk::{Runtime, ProgramBuilder};
//!
//! # fn main() -> anyhow::Result<()> {
//! let program = ProgramBuilder::new()
//!     .permission("io.print")
//!     .line(r#".func main"#)
//!     .line(r#"PUSH_STR "hello, cvm1""#)
//!     .line(r#"CAP_CALL "io.print" 1"#)
//!     .line(r#"HALT"#)
//!     .build()?;
//!
//! let result = Runtime::new().run(&program)?;
//! assert_eq!(result.output, "hello, cvm1");
//! # Ok(()) }
//! ```

pub mod builder;
pub mod compile;
pub mod caps;
pub mod compute;
pub mod host_caps;
pub mod bus;
pub mod task;
pub mod akg;
#[cfg(feature = "net")]
pub mod net;
#[cfg(feature = "db")]
pub mod db;
#[cfg(feature = "graphics")]
pub mod graphics;
pub mod stdlib;
pub mod runtime;
pub mod repl;

// Re-export the core crush-vm types a host author needs.
pub use crush_vm::{assemble, disassemble, Program, Quotas, VmError, VmResult};
pub use crush_vm::vm::Value;
pub use crush_vm::run as run_program;
pub use crush_vm::{HostCap, HostCapSpec, HostCaps, run_with_caps};

pub use builder::{ProgramBuilder, ProgramBuilderError};
pub use caps::{CapabilityError, print, concat, len};
pub use compute::{CrushEngine, CrushJob, CrushOutcome};
pub use host_caps::{HostCapsBuilder, FsReadCap, FsWriteCap, FsExistsCap, FsListCap, EnvGetCap, TimeNowCap, ProcessExecCap, CryptoSha256Cap, CryptoRandomCap};
#[cfg(feature = "graphics")]
pub use graphics::{CanvasCreateCap, RectCap, CircleCap, TextCap, ToSvgCap};
pub use runtime::{Runtime, RuntimeError};

/// Current SDK version, kept in lock-step with the workspace version.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
