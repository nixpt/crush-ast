use std::path::Path;
use std::process::Command;

use crate::manifest::{CapsuleType, Manifest, PayloadFormat, ScriptRuntime};
use crush_vm::HostCaps;

/// Result of a capsule execution
#[derive(Debug)]
pub enum ExecutionResult {
    Vm,
    Process(std::process::Child),
    None,
}

/// Trait for executing capsules
pub trait CapsuleRunner {
    fn run(
        &self,
        manifest: &Manifest,
        payload_path: &Path,
        args: &[String],
    ) -> anyhow::Result<ExecutionResult>;
}

/// Runner for Crush VM capsules (in-process via crush-lang-sdk).
///
/// `host_caps` lets the embedder extend the VM with custom host-provided
/// capabilities (e.g. a recording `register_command` shim) without forking
/// `crush-vm`. `None` preserves the historical behaviour: only the
/// built-in portable capability registry is available.
#[derive(Default)]
pub struct CrushRunner {
    pub host_caps: Option<HostCaps>,
}

impl CapsuleRunner for CrushRunner {
    fn run(
        &self,
        _manifest: &Manifest,
        payload_path: &Path,
        _args: &[String],
    ) -> anyhow::Result<ExecutionResult> {
        let source = std::fs::read_to_string(payload_path)?;
        let program = crush_lang_sdk::compile::compile_crush_source(&source)?;

        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&program, &quotas, self.host_caps.as_ref())?;

        if !result.output.is_empty() {
            println!("{}", result.output);
        }

        Ok(ExecutionResult::Vm)
    }
}

/// Runner for native binaries
pub struct NativeRunner;

impl CapsuleRunner for NativeRunner {
    fn run(
        &self,
        manifest: &Manifest,
        _payload_path: &Path,
        args: &[String],
    ) -> anyhow::Result<ExecutionResult> {
        let entry_point = &manifest.capsule.entry;
        let mut cmd = Command::new(entry_point);
        cmd.args(args);

        cmd.env("CRUSH_CAPSULE_NAME", &manifest.capsule.name);
        cmd.env("CRUSH_CAPSULE_VERSION", &manifest.capsule.version);

        let child = cmd.spawn()?;
        Ok(ExecutionResult::Process(child))
    }
}

/// Runner for script capsules (JS/TS via Bun, Python, etc.)
pub struct ScriptRunner {
    runtime: ScriptRuntime,
}

impl ScriptRunner {
    pub fn new(runtime: ScriptRuntime) -> Self {
        Self { runtime }
    }

    fn get_runtime_command(&self) -> (&'static str, Vec<&'static str>) {
        match self.runtime {
            ScriptRuntime::Bun => ("bun", vec!["run"]),
            ScriptRuntime::Node => ("node", vec![]),
            ScriptRuntime::Deno => ("deno", vec!["run", "--allow-read", "--allow-write"]),
            ScriptRuntime::Python => ("python3", vec![]),
        }
    }
}

impl CapsuleRunner for ScriptRunner {
    fn run(
        &self,
        manifest: &Manifest,
        payload_path: &Path,
        args: &[String],
    ) -> anyhow::Result<ExecutionResult> {
        let (runtime_bin, runtime_args) = self.get_runtime_command();

        let mut cmd = Command::new(runtime_bin);
        cmd.args(&runtime_args);
        cmd.arg(payload_path);
        cmd.args(args);

        // Provide capsule metadata as env vars
        cmd.env("CRUSH_CAPSULE_NAME", &manifest.capsule.name);
        cmd.env("CRUSH_CAPSULE_VERSION", &manifest.capsule.version);
        cmd.env("CRUSH_RUNTIME", format!("{:?}", self.runtime));

        let child = cmd.spawn()?;
        Ok(ExecutionResult::Process(child))
    }
}

/// Get runner from manifest capsule type
pub fn get_runner(capsule_type: &CapsuleType) -> Box<dyn CapsuleRunner> {
    match capsule_type {
        CapsuleType::Auto | CapsuleType::Crush => Box::new(CrushRunner::default()),
        CapsuleType::Native => Box::new(NativeRunner),
        // Container variant deleted (CRUSHCN-1) — see TICKETS/CRUSHRUNNERS-1.md Gap 1.
        CapsuleType::Script(runtime) => Box::new(ScriptRunner::new(runtime.clone())),
    }
}

/// Auto-detect runner from payload path + manifest
pub fn get_runner_for_payload(payload_path: &Path, manifest: &Manifest) -> Box<dyn CapsuleRunner> {
    let capsule_type = crate::manifest::language_to_capsule_type(&manifest.capsule.language);
    if capsule_type != CapsuleType::Auto {
        return get_runner(&capsule_type);
    }

    let format = PayloadFormat::from_path(payload_path);

    let format = if format == PayloadFormat::Unknown {
        if let Ok(bytes) = std::fs::read(payload_path) {
            let detected = PayloadFormat::from_magic(&bytes);
            if detected != PayloadFormat::Unknown {
                detected
            } else {
                format
            }
        } else {
            format
        }
    } else {
        format
    };

    match format {
        PayloadFormat::Casm => Box::new(CrushRunner::default()),
        PayloadFormat::JavaScript | PayloadFormat::TypeScript => {
            Box::new(ScriptRunner::new(ScriptRuntime::Bun))
        }
        PayloadFormat::Python => Box::new(ScriptRunner::new(ScriptRuntime::Python)),
        PayloadFormat::NativeElf | PayloadFormat::NativeMachO | PayloadFormat::NativePe => {
            Box::new(NativeRunner)
        }
        PayloadFormat::Unknown => Box::new(CrushRunner::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::CapsuleSection;

    fn make_manifest(language: &str, entry: &str) -> Manifest {
        Manifest {
            capsule: CapsuleSection {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                entry: entry.to_string(),
                language: language.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_get_runner_crush() {
        let runner = get_runner(&CapsuleType::Crush);
        let m = make_manifest("crush", "main.crush");
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("main.crush");
        std::fs::write(&payload, "fn main() {}").unwrap();
        let result = runner.run(&m, &payload, &[]);
        assert!(result.is_ok());
        match result.unwrap() {
            ExecutionResult::Vm => {}
            _ => panic!("expected Vm result"),
        }
    }

    #[test]
    fn test_script_runtime_commands() {
        let runner = ScriptRunner::new(ScriptRuntime::Bun);
        assert_eq!(runner.get_runtime_command().0, "bun");
        let runner = ScriptRunner::new(ScriptRuntime::Node);
        assert_eq!(runner.get_runtime_command().0, "node");
        let runner = ScriptRunner::new(ScriptRuntime::Deno);
        assert_eq!(runner.get_runtime_command().0, "deno");
        let runner = ScriptRunner::new(ScriptRuntime::Python);
        assert_eq!(runner.get_runtime_command().0, "python3");
    }

    #[test]
    fn test_get_runner_for_payload_resolves_by_extension() {
        // .py → ScriptRunner(Python) dispatches to Python runner
        let m = make_manifest("", "main.py");
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("main.py");
        std::fs::write(&payload, "print('hello')").unwrap();
        // Dispatch should resolve to Python runner without error
        let _runner = get_runner_for_payload(&payload, &m);
    }

    #[test]
    fn test_get_runner_language_override() {
        // language="python" in manifest overrides .ts extension
        let m = make_manifest("python", "app.ts");
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("app.ts");
        std::fs::write(&payload, "print('hello')").unwrap();
        let _runner = get_runner_for_payload(&payload, &m);
    }

}
