//! Host runtime for the CVM1 bytecode VM.
//!
//! [`Runtime`] wraps [`crush_vm::run_with_caps`] with convenience methods for
//! loading programs from blobs or CASM text, applying quotas, registering host
//! capabilities, and inspecting results.

use crush_vm::{HostCaps, Program, Quotas, VmError, VmResult, assemble, run_with_caps};

/// Errors that can occur when running a program through the SDK.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("failed to load CVM1 blob: {0}")]
    LoadBlob(String),

    #[error("failed to assemble CASM source: {0}")]
    Assembly(String),

    #[error(transparent)]
    Vm(#[from] VmError),
}

/// A configured CVM1 host runtime.
///
/// The runtime does not hold mutable VM state; it carries execution quotas
/// and an optional host-capability registry. Programs are executed statelessly
/// and do not share state between runs.
#[derive(Debug)]
pub struct Runtime {
    quotas: Quotas,
    host_caps: Option<HostCaps>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Create a runtime with default quotas and no host capabilities.
    pub fn new() -> Self {
        Self {
            quotas: Quotas::default(),
            host_caps: None,
        }
    }

    /// Create a runtime from explicit quotas.
    pub fn with_quotas(quotas: Quotas) -> Self {
        Self {
            quotas,
            host_caps: None,
        }
    }

    /// Register host capabilities.
    pub fn with_host_caps(mut self, host_caps: HostCaps) -> Self {
        self.host_caps = Some(host_caps);
        self
    }

    /// Return the quotas used by this runtime.
    pub fn quotas(&self) -> &Quotas {
        &self.quotas
    }

    /// Replace the quotas used by this runtime.
    pub fn set_quotas(&mut self, quotas: Quotas) {
        self.quotas = quotas;
    }

    /// Run a pre-loaded [`Program`].
    pub fn run(&self, program: &Program) -> Result<VmResult, RuntimeError> {
        Ok(run_with_caps(
            program,
            &self.quotas,
            self.host_caps.as_ref(),
        )?)
    }

    /// Load a CVM1 binary blob and run it.
    pub fn run_blob(&self, blob: &[u8]) -> Result<VmResult, RuntimeError> {
        let program =
            Program::from_blob(blob).map_err(|e| RuntimeError::LoadBlob(e.to_string()))?;
        self.run(&program)
    }

    /// Assemble CASM text and run the resulting program.
    ///
    /// `permissions` lists the capability names that the program is allowed
    /// to invoke (e.g. `["io.print"]`).
    pub fn run_casm(
        &self,
        source: &str,
        permissions: &[&str],
        name: Option<&str>,
    ) -> Result<VmResult, RuntimeError> {
        let program = assemble(source, Some(permissions), name)
            .map_err(|e| RuntimeError::Assembly(e.to_string()))?;
        self.run(&program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_hello_program() {
        let source = r#"
            .func main
            PUSH_STR "hi"
            CAP_CALL "io.print" 1
            HALT
        "#;

        let result = Runtime::new()
            .run_casm(source, &["io.print"], Some("hello"))
            .expect("run should succeed");

        assert_eq!(result.output, "hi");
        assert!(result.halted);
    }

    #[test]
    fn missing_permission_is_caught() {
        let source = r#"
            .func main
            PUSH_STR "hi"
            CAP_CALL "io.print" 1
            HALT
        "#;

        let err = Runtime::new()
            .run_casm(source, &[], Some("no-perms"))
            .expect_err("should fail without permission");

        assert!(
            err.to_string().contains("capability not declared"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn quotas_are_applied() {
        let source = r#"
            .func main
            loop:
            JMP loop
            HALT
        "#;

        let mut quotas = Quotas::default();
        quotas.max_steps = 10;

        let err = Runtime::with_quotas(quotas)
            .run_casm(source, &[], Some("infinite-loop"))
            .expect_err("should hit step quota");

        assert!(
            err.to_string().contains("instruction quota exceeded"),
            "unexpected error: {err}"
        );
    }
}
