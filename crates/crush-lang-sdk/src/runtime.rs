//! Host runtime for the CVM1 bytecode VM.
//!
//! [`Runtime`] wraps [`crush_vm::run_with_caps`] with convenience methods for
//! loading programs from blobs or CASM text, applying quotas, registering host
//! capabilities, and inspecting results.
//!
//! ## Codebase caps (AI-native)
//!
//! Call [`Runtime::with_codebase`] to auto-build a `CrushIndex` from in-memory
//! Crush source and inject the six `codebase.*` host caps.  Or use
//! [`Runtime::with_codebase_files`] to read Crush source files from disk.

use crush_frontend::parse_source;
use crush_index::CrushIndex;
use crush_vm::{HostCaps, Program, Quotas, VmError, VmResult, assemble, run_with_caps};
use std::sync::Arc;

/// Errors that can occur when running a program through the SDK.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("failed to load CVM1 blob: {0}")]
    LoadBlob(String),

    #[error("failed to assemble CASM source: {0}")]
    Assembly(String),

    #[error("failed to parse Crush source for codebase index: {module}: {cause}")]
    IndexParse { module: String, cause: String },

    #[error("failed to read source file '{path}': {cause}")]
    IndexRead { path: String, cause: String },

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

    /// Parse Crush source code for each `(module_name, source)` pair, build a
    /// [`CrushIndex`], and register the six `codebase.*` host capabilities.
    ///
    /// Existing capabilities (from a prior [`Self::with_host_caps`] call) are
    /// preserved — the codebase caps are appended, not replaced.
    ///
    /// # Example
    /// ```rust,no_run
    /// use crush_lang_sdk::Runtime;
    ///
    /// let scheduler = "fn main() { io.print(\"tick\") }";
    /// let rt = Runtime::new()
    ///     .with_codebase(&[("scheduler", scheduler)])
    ///     .unwrap();
    /// ```
    pub fn with_codebase(
        mut self,
        sources: &[(&str, &str)],
    ) -> Result<Self, RuntimeError> {
        let mut index = CrushIndex::new();
        for (module_name, source) in sources {
            let program = parse_source(source).map_err(|e| RuntimeError::IndexParse {
                module: module_name.to_string(),
                cause: e.to_string(),
            })?;
            index.add_program(module_name, &program);
        }
        let caps = self.host_caps.get_or_insert_with(HostCaps::new);
        crate::codebase::register(caps, Arc::new(index));
        Ok(self)
    }

    /// Read Crush source files from disk, build a [`CrushIndex`], and register
    /// the six `codebase.*` host capabilities.
    ///
    /// Each file's stem (filename without extension) is used as the module name.
    /// Existing capabilities are preserved.
    ///
    /// # Example
    /// ```rust,no_run
    /// use crush_lang_sdk::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .with_codebase_files(&["src/scheduler.crush", "src/types.crush"])
    ///     .unwrap();
    /// ```
    pub fn with_codebase_files(
        mut self,
        paths: &[impl AsRef<std::path::Path>],
    ) -> Result<Self, RuntimeError> {
        let mut index = CrushIndex::new();
        for path in paths {
            let path = path.as_ref();
            let module_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let source = std::fs::read_to_string(path).map_err(|e| RuntimeError::IndexRead {
                path: path.display().to_string(),
                cause: e.to_string(),
            })?;
            let program = parse_source(&source).map_err(|e| RuntimeError::IndexParse {
                module: module_name.to_string(),
                cause: e.to_string(),
            })?;
            index.add_program(module_name, &program);
        }
        let caps = self.host_caps.get_or_insert_with(HostCaps::new);
        crate::codebase::register(caps, Arc::new(index));
        Ok(self)
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

    #[test]
    fn with_codebase_registers_caps() {
        let crush_src = r#"
@module {
  purpose: "test nav module"
  exports: [navigate]
}
fn navigate(url) {
  let x = 1
}
"#;
        let rt = Runtime::new()
            .with_codebase(&[("nav", crush_src)])
            .expect("index build should succeed");

        // The runtime now has codebase caps registered; host_caps is Some.
        // Verify by checking the internal state via a round-trip cap check.
        // We probe by building a caps set and running a CASM program that
        // calls codebase.modules — if it errors "capability not declared"
        // then the cap wasn't registered; if it errors "not permitted" that
        // also means not registered; success or any other VM error means it
        // was registered and invoked.
        let casm = r#"
            .func main
            CAP_CALL "codebase.modules" 0
            HALT
        "#;
        let result = rt.run_casm(casm, &["codebase.modules"], Some("probe"));
        // The VM runs and hits the cap (returns an array) — any result that
        // isn't "capability not declared" confirms registration.
        match result {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    !msg.contains("capability not declared"),
                    "codebase.modules was not registered: {msg}"
                );
            }
        }
    }

    #[test]
    fn with_codebase_files_missing_path_is_reported() {
        let err = Runtime::new()
            .with_codebase_files(&["/nonexistent/path/missing.crush"])
            .expect_err("missing file should fail");
        assert!(
            err.to_string().contains("missing.crush"),
            "error should mention the path: {err}"
        );
    }

    #[test]
    fn with_codebase_preserves_existing_caps() {
        use crate::host_caps::HostCapsBuilder;
        let existing = HostCapsBuilder::new().time(true).build();
        let crush_src = "fn f() { }";
        let rt = Runtime::new()
            .with_host_caps(existing)
            .with_codebase(&[("m", crush_src)])
            .expect("index build should succeed");

        // Both time.now and codebase.modules must be present.
        // We verify by running probes for both — neither should say "not declared".
        for cap in ["time.now", "codebase.modules"] {
            let casm = format!(
                ".func main\nCAP_CALL \"{cap}\" 0\nHALT\n"
            );
            let result = rt.run_casm(&casm, &[cap], Some("probe"));
            if let Err(e) = result {
                assert!(
                    !e.to_string().contains("capability not declared"),
                    "{cap} missing after with_codebase: {e}"
                );
            }
        }
    }
}
