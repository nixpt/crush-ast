//! `crush doctor` — which host runtimes polyglot blocks can actually reach.
//!
//! `@python { }` / `@javascript { }` / `@bash { }` spawn a host interpreter
//! (`EXEC_LANG`), so a missing `node` only shows up when a block fails. This
//! module looks each one up on `PATH` the way the spawn will — binary names come
//! from [`crush_vm::resolve_lang_binary`], the same allowlist `EXEC_LANG` uses —
//! and asks it for its version. It also reports the sandbox tooling
//! (`bwrap`, `buckets`) and which polyglot features this build was compiled with.
//!
//! Nothing here runs a Crush program or grants a capability: it is a host-side
//! report, read-only apart from running `<tool> --version`.

use serde::Serialize;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// The languages `crush run --polyglot` grants (`crush-run.rs`). A missing
/// interpreter for one of these makes the report fail.
pub const POLYGLOT_LANGS: [&str; 3] = ["python", "javascript", "bash"];

/// How long a `--version` probe may take before it is killed.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// One host tool the doctor looked for.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCheck {
    /// What needs it: a polyglot language (`python`) or `sandbox`.
    pub role: String,
    /// Executable name searched for on `PATH`.
    pub binary: String,
    /// Where it was found, if anywhere.
    pub path: Option<PathBuf>,
    /// First line of `<binary> --version`, if it answered.
    pub version: Option<String>,
    /// Whether a missing tool fails the report (exit code 1).
    pub required: bool,
}

impl ToolCheck {
    pub fn found(&self) -> bool {
        self.path.is_some()
    }
}

/// Cargo features of this build that change polyglot behaviour.
#[derive(Debug, Clone, Serialize)]
pub struct BuildFeatures {
    /// `@python` free-variable marshaling (`polyglot-python`).
    pub polyglot_python: bool,
    /// `@javascript` free-variable marshaling (`polyglot-javascript`).
    pub polyglot_javascript: bool,
    /// `@lang[deps]` blocks run in a buckets + bwrap sandbox (crush-vm `sandboxed-polyglot`).
    pub sandboxed_polyglot: bool,
}

impl BuildFeatures {
    pub fn current() -> Self {
        BuildFeatures {
            polyglot_python: cfg!(feature = "polyglot-python"),
            polyglot_javascript: cfg!(feature = "polyglot-javascript"),
            sandboxed_polyglot: crush_vm::SANDBOXED_POLYGLOT,
        }
    }
}

/// The full report printed by `crush doctor`.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub version: String,
    pub features: BuildFeatures,
    pub tools: Vec<ToolCheck>,
    /// False when any required tool is missing.
    pub ok: bool,
}

impl Report {
    /// Check every tool against `path_var` (a `PATH`-style list; `None` = the
    /// process's own `PATH`).
    pub fn collect(path_var: Option<&OsStr>) -> Self {
        let features = BuildFeatures::current();
        let owned_path = std::env::var_os("PATH");
        let path_var = path_var.or(owned_path.as_deref());

        let mut tools = Vec::new();
        for lang in POLYGLOT_LANGS {
            let (binary, _flag) = crush_vm::resolve_lang_binary(lang)
                .expect("every POLYGLOT_LANGS entry is in EXEC_LANG's allowlist");
            tools.push(check(lang, binary, true, path_var));
        }
        // bwrap is what the sandboxed build actually spawns; the `buckets` CLI
        // is the user-facing tool for priming its cache. Neither is needed by
        // a build without `sandboxed-polyglot`.
        tools.push(check(
            "sandbox",
            "bwrap",
            features.sandboxed_polyglot,
            path_var,
        ));
        tools.push(check("sandbox", "buckets", false, path_var));

        let ok = tools.iter().all(|t| t.found() || !t.required);
        Report {
            version: env!("CARGO_PKG_VERSION").to_string(),
            features,
            tools,
            ok,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Report serializes")
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("crush {} — polyglot runtime check\n\n", self.version);
        for t in &self.tools {
            let mark = match (t.found(), t.required) {
                (true, _) => "ok     ",
                (false, true) => "MISSING",
                (false, false) => "absent ",
            };
            let detail = match (&t.path, &t.version) {
                (Some(p), Some(v)) => format!("{v} ({})", p.display()),
                (Some(p), None) => format!("version unknown ({})", p.display()),
                (None, _) if t.required => "not found on PATH".to_string(),
                (None, _) => "not found on PATH (optional)".to_string(),
            };
            out.push_str(&format!(
                "  {mark}  {:<10} {:<8} {detail}\n",
                t.role, t.binary
            ));
        }
        let f = &self.features;
        out.push_str(&format!(
            "\nbuild: polyglot-python={} polyglot-javascript={} sandboxed-polyglot={}\n",
            f.polyglot_python, f.polyglot_javascript, f.sandboxed_polyglot
        ));
        out.push_str(if self.ok {
            "\nall required runtimes found\n"
        } else {
            "\nsome required runtimes are missing — polyglot blocks for them will fail\n"
        });
        out
    }
}

fn check(role: &str, binary: &str, required: bool, path_var: Option<&OsStr>) -> ToolCheck {
    let path = path_var.and_then(|p| find_on_path(binary, p));
    let version = path.as_deref().and_then(probe_version);
    ToolCheck {
        role: role.to_string(),
        binary: binary.to_string(),
        path,
        version,
        required,
    }
}

/// First executable file named `binary` in a `PATH`-style list.
pub fn find_on_path(binary: &str, path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Start `<path> --version`, retrying briefly while the file is "text file
/// busy" (ETXTBSY): exec fails that way while any process still has the file
/// open for writing — a tool mid-upgrade, or, in tests, a just-written script
/// whose write descriptor a concurrently forked child inherited until its exec.
fn spawn_version(path: &Path) -> Option<std::process::Child> {
    let mut attempts = 0;
    loop {
        match Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => return Some(child),
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempts < 20 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
}

/// Run `<path> --version` and keep the first non-empty line of stdout (or
/// stderr — some tools print it there). Killed after [`VERSION_TIMEOUT`].
fn probe_version(path: &Path) -> Option<String> {
    let mut child = spawn_version(path)?;
    let pid = child.id();
    let stdout = child.stdout.take()?;
    let stderr = child.stderr.take()?;
    let (tx, rx) = mpsc::channel();
    // Drain both pipes concurrently so a chatty stderr can't block stdout.
    std::thread::spawn(move || {
        use std::io::Read;
        let err_reader = std::thread::spawn(move || {
            let mut err = String::new();
            let _ = { stderr }.read_to_string(&mut err);
            err
        });
        let mut out = String::new();
        let _ = { stdout }.read_to_string(&mut out);
        let err = err_reader.join().unwrap_or_default();
        let _ = tx.send((out, err));
    });
    let Ok((out, err)) = rx.recv_timeout(VERSION_TIMEOUT) else {
        let _ = child.kill();
        let _ = child.wait();
        eprintln!(
            "crush doctor: '{}' --version (pid {pid}) timed out",
            path.display()
        );
        return None;
    };
    let _ = child.wait();
    out.lines()
        .chain(err.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_tool(dir: &Path, name: &str, script: &str) {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn finds_executables_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("node"), "not executable").unwrap();
        fake_tool(dir.path(), "python3", "echo 'Python 3.99.1'");
        let path = dir.path().as_os_str();
        assert!(find_on_path("node", path).is_none());
        assert_eq!(
            find_on_path("python3", path),
            Some(dir.path().join("python3"))
        );
    }

    #[test]
    fn version_falls_back_to_stderr() {
        let dir = tempfile::tempdir().unwrap();
        fake_tool(
            dir.path(),
            "bash",
            "echo >&2; echo 'GNU bash, version 9.9' >&2",
        );
        assert_eq!(
            probe_version(&dir.path().join("bash")).as_deref(),
            Some("GNU bash, version 9.9")
        );
    }

    #[test]
    fn missing_required_runtime_fails_the_report() {
        let dir = tempfile::tempdir().unwrap();
        fake_tool(dir.path(), "python3", "echo 'Python 3.99.1'");
        fake_tool(dir.path(), "bash", "echo 'GNU bash, version 9.9'");
        let report = Report::collect(Some(dir.path().as_os_str()));
        assert!(!report.ok);
        let node = report.tools.iter().find(|t| t.binary == "node").unwrap();
        assert!(node.required && !node.found());
        let py = report.tools.iter().find(|t| t.binary == "python3").unwrap();
        assert_eq!(py.version.as_deref(), Some("Python 3.99.1"));
    }

    #[test]
    fn optional_sandbox_tools_do_not_fail_the_report() {
        let dir = tempfile::tempdir().unwrap();
        for (name, v) in [
            ("python3", "Python 3.99.1"),
            ("node", "v99.0.0"),
            ("bash", "GNU bash, version 9.9"),
        ] {
            fake_tool(dir.path(), name, &format!("echo '{v}'"));
        }
        let report = Report::collect(Some(dir.path().as_os_str()));
        // bwrap is required only in a sandboxed-polyglot build.
        assert_eq!(report.ok, !crush_vm::SANDBOXED_POLYGLOT);
        assert!(
            report
                .tools
                .iter()
                .any(|t| t.binary == "buckets" && !t.found() && !t.required)
        );
    }
}
