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
        let ext = payload_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let program = if ext == "casm" || ext == "sno" {
            let content = std::fs::read_to_string(payload_path)?;
            let casm_program: ::casm::Program = serde_json::from_str(&content)?;
            crush_lang_sdk::compile::casm_to_vm(&casm_program)?
        } else {
            let source = std::fs::read_to_string(payload_path)?;
            crush_lang_sdk::compile::compile_crush_source(&source)?
        };

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
            ScriptRuntime::Sona => ("sona", vec!["run"]),
        }
    }

    /// The `buckets` alias for this runtime, if buckets knows how to
    /// provision it. `Sona` has no bottle in buckets' index (it's a
    /// crush-specific tool, not something pkgx/buckets distributes), so it
    /// has no buckets-backed path and always falls back to the host PATH.
    fn buckets_spec(&self) -> Option<&'static str> {
        match self.runtime {
            ScriptRuntime::Bun => Some("bun"),
            ScriptRuntime::Node => Some("node"),
            ScriptRuntime::Deno => Some("deno"),
            ScriptRuntime::Python => Some("python"),
            ScriptRuntime::Sona => None,
        }
    }

    /// Resolve this runtime through buckets: a pinned toolchain version,
    /// installed to `~/.buckets/`, run under `bwrap` (falling back to
    /// unsandboxed if `bwrap` isn't on PATH — see `buckets::sandbox`).
    /// `cwd` is bound read-write (the sandbox's fresh mount namespace needs
    /// it to exist, and scripts commonly read/write files beside themselves).
    /// `runtime_version` is the manifest's `capsule.runtime_version`, if
    /// set — appended as `"{alias}@{version}"` so the resolved toolchain is
    /// actually pinned, not just "latest".
    ///
    /// Returns `None` if this runtime has no buckets spec, or if resolution
    /// fails (offline, unknown alias, bottle fetch error, ...) — callers
    /// fall back to a bare host-PATH `Command` in either case, so a
    /// network hiccup degrades a capsule run rather than breaking it.
    fn buckets_command(
        &self,
        runtime_bin: &str,
        args: &[String],
        cwd: &Path,
        runtime_version: Option<&str>,
    ) -> Option<Command> {
        let alias = self.buckets_spec()?;
        let spec = match runtime_version {
            Some(v) => format!("{alias}@{v}"),
            None => alias.to_string(),
        };
        let config = buckets::Config::default();
        let index = buckets::Index::builtin();
        let resolved = match buckets::resolve(&spec, &config, &index) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("crush-pkg: buckets resolution for '{spec}' failed ({e}), falling back to host {runtime_bin}");
                return None;
            }
        };
        let profile = buckets::sandbox::SandboxProfile {
            project_dir: Some(cwd.to_path_buf()),
            extra_ro_binds: resolved.installations.iter().map(|i| i.path.clone()).collect(),
            allow_network: false,
        };
        Some(buckets::sandbox::sandboxed_command(
            runtime_bin,
            args,
            cwd,
            &resolved.env,
            &profile,
        ))
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

        let mut full_args: Vec<String> = runtime_args.iter().map(|s| s.to_string()).collect();
        full_args.push(payload_path.display().to_string());
        full_args.extend(args.iter().cloned());

        let cwd = payload_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);

        let mut cmd = self
            .buckets_command(runtime_bin, &full_args, &cwd, manifest.capsule.runtime_version.as_deref())
            .unwrap_or_else(|| {
                let mut c = Command::new(runtime_bin);
                c.args(&full_args);
                c
            });

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
        PayloadFormat::Sona => Box::new(ScriptRunner::new(ScriptRuntime::Sona)),
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
    fn test_buckets_spec_mapping() {
        assert_eq!(ScriptRunner::new(ScriptRuntime::Bun).buckets_spec(), Some("bun"));
        assert_eq!(ScriptRunner::new(ScriptRuntime::Node).buckets_spec(), Some("node"));
        assert_eq!(ScriptRunner::new(ScriptRuntime::Deno).buckets_spec(), Some("deno"));
        assert_eq!(ScriptRunner::new(ScriptRuntime::Python).buckets_spec(), Some("python"));
        // Sona has no pkgx/buckets bottle — always falls back to the host PATH.
        assert_eq!(ScriptRunner::new(ScriptRuntime::Sona).buckets_spec(), None);
    }

    #[test]
    fn test_buckets_command_returns_none_for_sona() {
        let runner = ScriptRunner::new(ScriptRuntime::Sona);
        let dir = tempfile::tempdir().unwrap();
        assert!(runner.buckets_command("sona", &[], dir.path(), None).is_none());
    }

    /// Exercises the real buckets resolve→install→sandbox pipeline against
    /// the network + `bwrap` — same reason `buckets`'s own test suite
    /// gates its download-dependent tests behind `#[ignore]`. Run with
    /// `cargo test -p crush-pkg -- --ignored` when validating this path
    /// for real (a fresh box with no `~/.buckets` cache and network access).
    #[test]
    #[ignore]
    fn test_script_runner_run_uses_buckets_for_python() {
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("hello.py");
        std::fs::write(&payload, "print('hello from buckets')").unwrap();

        let manifest = make_manifest("python", "hello.py");
        let runner = ScriptRunner::new(ScriptRuntime::Python);
        let result = runner.run(&manifest, &payload, &[]).expect("run");
        match result {
            ExecutionResult::Process(mut child) => {
                let status = child.wait().expect("wait");
                assert!(status.success());
            }
            _ => panic!("expected Process result"),
        }
    }

    /// Real pinning, not just "latest": the script asserts its own
    /// interpreter version and fails (nonzero exit) if buckets resolved the
    /// wrong one — self-verifying, so this doesn't need stdout capture
    /// plumbing `ScriptRunner::run` doesn't otherwise have.
    ///
    /// Uses `~3.11` (tilde, real minor-pin range ">=3.11.0, <3.12.0"), not
    /// bare `3.11` — found live while writing this test that
    /// `PackageReq::parse` (buckets' own spec grammar) turns ANY bare
    /// numeric version string into a caret range (`^3.11`, satisfied by
    /// 3.14.x too), regardless of component count. A caller wanting a real
    /// minor/patch pin needs an explicit `~`/`=` prefix in `runtime_version`.
    #[test]
    #[ignore]
    fn test_script_runner_honors_runtime_version_pin() {
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("check_version.py");
        std::fs::write(
            &payload,
            "import sys\nassert sys.version_info[:2] == (3, 11), sys.version\n",
        )
        .unwrap();

        let mut manifest = make_manifest("python", "check_version.py");
        manifest.capsule.runtime_version = Some("~3.11".to_string());
        let runner = ScriptRunner::new(ScriptRuntime::Python);
        let result = runner.run(&manifest, &payload, &[]).expect("run");
        match result {
            ExecutionResult::Process(mut child) => {
                let status = child.wait().expect("wait");
                assert!(status.success(), "pinned-version script failed: {status:?}");
            }
            _ => panic!("expected Process result"),
        }
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
