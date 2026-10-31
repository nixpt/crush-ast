//! Compile, preflight and run one program with the `ui.*` capabilities.
//!
//! The same path serves the CLI, the desktop backend and the headless tests,
//! so "refused without `--ui`" means one thing everywhere.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use crush_lang_sdk::effects::{MissingGrant, describe_missing};
use crush_lang_sdk::{HostCapsBuilder, Runtime, RuntimeError};
use crush_vm::{Quotas, VmError};

use crate::backend::UiBackend;
use crate::caps;

/// What the person running the program grants. Mirrors `crush-run`'s flags
/// (minus `--gpu`, `--graphics` and `--db`), plus `--ui`.
#[derive(Debug, Clone)]
pub struct Grants {
    /// `--ui`: the `ui.*` capabilities.
    pub ui: bool,
    pub fs: bool,
    pub fs_root: PathBuf,
    pub env: bool,
    pub time: bool,
    pub bus: bool,
    pub task: bool,
    pub akg: bool,
    pub process: bool,
    pub crypto: bool,
    pub net: bool,
    pub polyglot: bool,
    /// The program's own arguments (`sys.args()`).
    pub args: Vec<String>,
    /// `None` keeps the VM default.
    pub max_steps: Option<usize>,
    pub max_output: Option<usize>,
}

impl Default for Grants {
    fn default() -> Self {
        Self {
            ui: false,
            fs: false,
            fs_root: PathBuf::from("."),
            env: false,
            time: false,
            bus: false,
            task: false,
            akg: false,
            process: false,
            crypto: false,
            net: false,
            polyglot: false,
            args: Vec::new(),
            max_steps: None,
            max_output: None,
        }
    }
}

/// Where the program comes from.
pub enum Source {
    File(PathBuf),
    Text(String),
}

/// The program can request capabilities this run does not grant; nothing
/// was run. Display is the full explanation.
#[derive(Debug)]
pub struct NotGranted {
    pub text: String,
}

impl std::fmt::Display for NotGranted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::error::Error for NotGranted {}

/// How a program that ran came to an end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// Ran to the end.
    Finished,
    /// `sys.exit(code)`.
    Exit(i32),
}

/// The default step budget for a GUI program. A UI loop is long-lived, so the
/// VM's 1M-instruction default would end any real session; this is still a
/// bound, and `--max-steps` changes it.
pub const DEFAULT_MAX_STEPS: usize = 500_000_000;

fn load(source: &Source) -> Result<(crush_vm::Program, String)> {
    match source {
        Source::Text(text) => Ok((
            crush_lang_sdk::compile::compile_crush_source(text)?,
            "<text>".into(),
        )),
        Source::File(path) => {
            let name = path.display().to_string();
            match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
                "crush" => {
                    let text = std::fs::read_to_string(path)?;
                    Ok((crush_lang_sdk::compile::compile_crush_source(&text)?, name))
                }
                "cvm1" | "cvm" => Ok((crush_vm::Program::from_blob(&std::fs::read(path)?)?, name)),
                other => anyhow::bail!(
                    "unsupported file extension: {other} (expected .crush or .cvm1 / .cvm)"
                ),
            }
        }
    }
}

fn runtime(grants: &Grants, backend: Arc<dyn UiBackend>) -> Runtime {
    let defaults = Quotas::default();
    let quotas = Quotas {
        max_steps: grants.max_steps.unwrap_or(DEFAULT_MAX_STEPS),
        max_output: grants.max_output.unwrap_or(defaults.max_output),
        ..defaults
    };
    let mut caps = HostCapsBuilder::new()
        .polyglot(if grants.polyglot {
            &["python", "javascript", "bash"]
        } else {
            &[]
        })
        .fs(grants.fs)
        .fs_root(grants.fs_root.to_string_lossy())
        .env(grants.env)
        .time(grants.time)
        .bus(grants.bus)
        .task(grants.task)
        .akg(grants.akg)
        .process(grants.process)
        .crypto(grants.crypto)
        .net(grants.net)
        .stdlib(true)
        .args(grants.args.clone())
        .build();
    if grants.ui {
        caps::register(&mut caps, backend);
    }
    Runtime::with_quotas(quotas).with_host_caps(caps)
}

/// The refusal for `missing`, naming `--ui` for the `ui.*` family (which the
/// SDK's grant catalog cannot know about: it lives below this crate).
fn refusal(source: &str, mut missing: Vec<MissingGrant>) -> NotGranted {
    for m in &mut missing {
        if caps::is_ui_cap(&m.capability) {
            m.grant = Some(caps::GRANT);
        }
    }
    NotGranted {
        text: format!(
            "{source} needs capabilities this run does not grant; nothing was run:\n{}",
            describe_missing(&missing)
        ),
    }
}

/// Compile `source`, refuse it up front if it names a capability that
/// `grants` does not provide, then run it. Output goes to `out` as the
/// program prints it. A `NotGranted` error means nothing ran.
pub fn execute(
    source: &Source,
    grants: &Grants,
    backend: Arc<dyn UiBackend>,
    out: &mut dyn FnMut(&str),
) -> Result<RunEnd> {
    let (program, name) = load(source)?;
    for warning in &program.warnings {
        eprintln!("warning: {warning}");
    }
    let runtime = runtime(grants, backend);
    let missing = runtime.missing_grants(&program)?;
    if !missing.is_empty() {
        return Err(refusal(&name, missing).into());
    }
    match runtime.run_streaming(&program, out) {
        Ok(_) => Ok(RunEnd::Finished),
        Err(RuntimeError::Vm(VmError::Exit(code))) => Ok(RunEnd::Exit(code)),
        Err(e) => Err(e.into()),
    }
}

/// Is `path` a program this crate can run?
pub fn is_program(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("crush" | "cvm1" | "cvm")
    )
}
