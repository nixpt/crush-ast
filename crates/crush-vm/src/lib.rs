#[cfg(test)]
mod tests;

pub mod memory;
pub mod fastvm;
pub mod value;
pub mod assembler;
pub mod bytecode;
pub mod caps;
pub mod host;
pub mod portable_vm;
pub mod scheduler;
pub mod vm;

pub use assembler::{AssemblyError, assemble, disassemble};
pub use bytecode::Program;
pub use caps::{CapabilitySpec, capabilities, is_privileged as cap_is_privileged};
pub use host::{HostCap, HostCapSpec, HostCaps};
pub use portable_vm::{Frame, PortableVm, VmYield, value_to_text};
pub use vm::{Quotas, VmError, VmResult, run, run_with_caps, run_fastvm, run_fastvm_with_caps};

pub use memory::{Arena, Object};
pub use value::RuntimeValue;

pub mod ai_optimizer;
pub mod plugin;
pub mod cargo_cap;
pub mod c_api;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bad magic — not a CVM1 program")]
    BadMagic,
    #[error("unsupported CVM1 version: {0}")]
    UnsupportedVersion(u8),
    #[error("truncated blob")]
    Truncated,
    #[error("malformed manifest: {0}")]
    BadManifest(String),
}
