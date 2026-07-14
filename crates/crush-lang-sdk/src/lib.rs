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

pub mod akg;
pub mod builder;
pub mod bus;
pub mod caps;
pub mod cli;
pub mod codebase;
pub mod compile;
pub mod compute;
pub mod differential;
#[cfg(feature = "db")]
pub mod db;
#[cfg(any(feature = "db", feature = "stdlib"))]
mod util;
#[cfg(feature = "graphics")]
pub mod graphics;
pub mod host_caps;
#[cfg(feature = "net")]
pub mod net;
pub mod repl;
#[cfg(feature = "repl-helper")]
pub mod repl_helper;
pub mod repl_util;
pub mod runtime;
pub mod stdlib;
pub mod task;
pub mod theme;

// Re-export the core crush-vm types a host author needs.
pub use crush_vm::run as run_program;
pub use crush_vm::vm::Value;
pub use crush_vm::{HostCap, HostCapSpec, HostCaps, run_with_caps};
pub use crush_vm::{Program, Quotas, VmError, VmResult, assemble, disassemble};

pub use builder::{ProgramBuilder, ProgramBuilderError};
pub use caps::{CapabilityError, concat, len, print};
pub use cli::MessageFormat;
pub use compute::{CrushEngine, CrushJob, CrushOutcome};
#[cfg(feature = "graphics")]
pub use graphics::{CanvasCreateCap, CircleCap, RectCap, TextCap, ToSvgCap};
pub use host_caps::{
    CryptoRandomCap, CryptoSha256Cap, EnvGetCap, FsExistsCap, FsListCap, FsReadCap, FsWriteCap,
    HostCapsBuilder, ProcessExecCap, TimeNowCap,
};
pub use runtime::{Runtime, RuntimeError};

/// Current SDK version, kept in lock-step with the workspace version.
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
