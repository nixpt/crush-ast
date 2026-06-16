//! Compute-job runner for light-node integration.
//!
//! This module provides a sandboxed, resource-limited execution primitive that
//! light nodes (e.g. `mycelium-node` / `fabric`) can use to run untrusted
//! CRUSH/CVM1 bytecode tasks without pulling in the full Exosphere stack.

use std::time::Instant;

use crush_vm::{HostCaps, Program, Quotas, VmError, run_with_caps};

/// A single CVM1 compute job.
#[derive(Debug)]
pub struct CrushJob {
    /// CVM1 binary bytecode.
    pub bytecode: Vec<u8>,
    /// Capability permissions declared by the job.
    pub permissions: Vec<String>,
    /// Execution quotas.
    pub quotas: Quotas,
    /// Optional host-provided capabilities.
    pub host_caps: Option<HostCaps>,
}

impl CrushJob {
    /// Create a job from a compiled CVM1 blob.
    pub fn from_blob(bytecode: Vec<u8>) -> Self {
        Self {
            bytecode,
            permissions: Vec::new(),
            quotas: Quotas::default(),
            host_caps: None,
        }
    }

    /// Grant a capability permission.
    pub fn with_permission(mut self, cap: impl Into<String>) -> Self {
        self.permissions.push(cap.into());
        self
    }

    /// Set execution quotas.
    pub fn with_quotas(mut self, quotas: Quotas) -> Self {
        self.quotas = quotas;
        self
    }

    /// Attach host capabilities.
    pub fn with_host_caps(mut self, host_caps: HostCaps) -> Self {
        self.host_caps = Some(host_caps);
        self
    }
}

/// Outcome of a sandboxed CRUSH job.
#[derive(Debug, Clone)]
pub struct CrushOutcome {
    /// Whether the program ran to `HALT`.
    pub success: bool,
    /// Program stdout (collected from `io.print`).
    pub stdout: String,
    /// Optional error message if the VM failed or the program did not halt.
    pub error: Option<String>,
    /// Number of VM steps executed.
    pub steps: usize,
    /// Wall-clock execution time in milliseconds.
    pub wall_ms: u64,
}

impl CrushOutcome {
    /// Compute a content-addressed output hash for fraud detection / verification.
    pub fn output_hash(&self) -> String {
        sha256_hex(self.stdout.as_bytes())
    }
}

/// A sandboxed CRUSH compute engine.
///
/// The engine holds no mutable VM state; each job is executed independently.
#[derive(Debug, Default)]
pub struct CrushEngine;

impl CrushEngine {
    /// Create a new engine.
    pub fn new() -> Self {
        Self
    }

    /// Run a [`CrushJob`] and return the outcome.
    pub fn run(&self, job: &CrushJob) -> CrushOutcome {
        let start = Instant::now();

        let program = match Program::from_blob(&job.bytecode) {
            Ok(p) => p,
            Err(e) => {
                return CrushOutcome {
                    success: false,
                    stdout: String::new(),
                    error: Some(format!("invalid CVM1 blob: {e}")),
                    steps: 0,
                    wall_ms: start.elapsed().as_millis() as u64,
                };
            }
        };

        // Further restrict the program's declared permissions to the job's
        // explicit allow-list.
        let mut quotas = job.quotas.clone();
        if quotas.allowed_caps.is_none() {
            quotas.allowed_caps = Some(job.permissions.clone());
        }

        match run_with_caps(&program, &quotas, job.host_caps.as_ref()) {
            Ok(result) => CrushOutcome {
                success: result.halted,
                stdout: result.output.clone(),
                error: if result.halted { None } else { Some("program fell off end without HALT".into()) },
                steps: result.steps,
                wall_ms: start.elapsed().as_millis() as u64,
            },
            Err(VmError::CapNotDeclared(cap)) | Err(VmError::CapDenied(cap)) => CrushOutcome {
                success: false,
                stdout: String::new(),
                error: Some(format!("capability denied: {cap}")),
                steps: 0,
                wall_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => CrushOutcome {
                success: false,
                stdout: String::new(),
                error: Some(e.to_string()),
                steps: 0,
                wall_ms: start.elapsed().as_millis() as u64,
            },
        }
    }

    /// Assemble CASM text into a job payload.
    pub fn assemble_job(
        &self,
        source: &str,
        permissions: &[&str],
        quotas: Quotas,
    ) -> Result<CrushJob, String> {
        let program = crate::assemble(source, Some(permissions), None)
            .map_err(|e| e.to_string())?;
        Ok(CrushJob {
            bytecode: program.to_blob(),
            permissions: permissions.iter().map(|s| s.to_string()).collect(),
            quotas,
            host_caps: None,
        })
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProgramBuilder;

    fn hello_job() -> CrushJob {
        let program = ProgramBuilder::new()
            .permission("io.print")
            .line(".func main")
            .line(r#"PUSH_STR "hello, job""#)
            .line(r#"CAP_CALL "io.print" 1"#)
            .line("HALT")
            .build()
            .expect("build");

        CrushJob::from_blob(program.to_blob()).with_permission("io.print")
    }

    #[test]
    fn engine_runs_job() {
        let outcome = CrushEngine::new().run(&hello_job());
        assert!(outcome.success);
        assert_eq!(outcome.stdout, "hello, job");
        assert!(outcome.error.is_none());
        assert!(outcome.steps > 0);
    }

    #[test]
    fn engine_denies_missing_permission() {
        let mut job = hello_job();
        job.permissions.clear();

        let outcome = CrushEngine::new().run(&job);
        assert!(!outcome.success);
        assert!(outcome.error.as_deref().unwrap().contains("capability denied"));
    }

    #[test]
    fn engine_enforces_step_quota() {
        let program = ProgramBuilder::new()
            .line(".func main")
            .line("loop:")
            .line("JMP loop")
            .line("HALT")
            .build()
            .expect("build");

        let mut quotas = Quotas::default();
        quotas.max_steps = 5;

        let job = CrushJob::from_blob(program.to_blob()).with_quotas(quotas);
        let outcome = CrushEngine::new().run(&job);
        assert!(!outcome.success);
        assert!(outcome.error.as_deref().unwrap().contains("instruction quota"));
    }

    #[test]
    fn outcome_hash_is_stable() {
        let outcome = CrushEngine::new().run(&hello_job());
        let h1 = outcome.output_hash();
        let h2 = outcome.output_hash();
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }
}
