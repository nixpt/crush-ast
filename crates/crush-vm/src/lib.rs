#[cfg(test)]
mod tests;

pub mod assembler;
pub mod bytecode;
pub mod caps;
pub mod vm;

pub use assembler::{AssemblyError, assemble, disassemble};
pub use bytecode::Program;
pub use caps::{CapabilitySpec, CAPABILITIES, PORTABLE_CAPS, is_privileged};
pub use vm::{Quotas, VmError, VmResult, run};

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
