#[cfg(test)]
mod tests;

pub mod memory;
// FastVM bakes ai_optimizer::VmOptimizer into its state unconditionally —
// same native-only gate as ai_optimizer itself.
#[cfg(feature = "native-plugins")]
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
pub use host::{HostCap, HostCapSpec, HostCaps, polyglot_gate};
pub use portable_vm::{Frame, PortableVm, VmYield, value_to_text};
pub use vm::{Quotas, VmError, VmResult, run, run_with_caps};
#[cfg(feature = "native-plugins")]
pub use vm::{run_fastvm, run_fastvm_with_caps, run_casm_json, CrushResultExt};

pub use memory::{Arena, Object};
pub use value::RuntimeValue;

#[cfg(feature = "native-plugins")]
pub mod ai_optimizer;
#[cfg(feature = "native-plugins")]
pub mod plugin;
// cargo_cap needs a real `cargo` binary to spawn (std::process::Command) —
// no wasm32 story either way, folded into the same native-only gate.
#[cfg(feature = "native-plugins")]
pub mod cargo_cap;
// C-ABI embedding runs on the FastVM hot path (run_fastvm) — native-only,
// same gate.
#[cfg(feature = "native-plugins")]
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
