//! CRUSH-20: the 4th, `buckets`-sandboxed `EXEC_LANG` execution path.
//!
//! Provisions an isolated, bwrap-sandboxed language runtime via the
//! `buckets` crate (a sibling project — see this crate's `Cargo.toml` dep
//! comment) instead of spawning the host's own interpreter with the host
//! process's full authority. Proven empirically for python/node/bash by the
//! CRUSHAST-BUCKETSPIKE-1/2 spikes (`SPIKE_RESULTS.md`/`SPIKE_RESULTS_2.md`
//! at the repo root, `crates/crush-bucketspike`) — this module is the
//! production wiring of that spike into the real `EXEC_LANG` opcode
//! handlers (`scheduler.rs`, `portable_vm.rs`).
//!
//! Gated behind the `sandboxed-polyglot` feature (off by default): when
//! disabled, `EXEC_LANG` falls back to today's plain `Command::new(binary)`
//! (see `scheduler::run_exec_lang`'s feature-gated branch).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use buckets::config::Config;
use buckets::index::Index;
use buckets::resolve::resolve_multi;
use buckets::sandbox::{sandboxed_command, SandboxProfile};
use buckets::types::ResolvedEnvironment;

/// Map a canonical `@lang` tag to the bare-runtime `buckets` spec used to
/// provision it. Sibling allowlist to `scheduler::resolve_lang_binary` —
/// buckets provisions bare language runtimes only (no PyPI/npm-level
/// dependency resolution; see CRUSH-20's "numpy reframe" non-goal), so this
/// always resolves to a plain `<project>@<constraint>` spec, never a
/// dependency-set. Version constraints are loose (`^`) to track buckets'
/// own resolvable range, matching the CRUSHAST-BUCKETSPIKE-1/2 spike specs.
pub(crate) fn lang_to_bucket_spec(lang: &str) -> Option<&'static str> {
    match lang {
        "python" | "python3" | "py" => Some("python@3.11"),
        "javascript" | "js" | "es6" | "ecmascript" | "node" => Some("node@20"),
        "bash" | "sh" => Some("bash@5"),
        _ => None,
    }
}

/// Resolve `specs` via buckets on a background thread, bounded by
/// `deadline_ms`. Mirrors `HostCap::call_with_deadline`'s (CRUSH-19)
/// cooperative-deadline shape for a provisioning step that isn't a
/// subprocess `scheduler::run_with_wall_clock_limit` can wrap directly: cold
/// resolve+fetch+install measured up to ~4.4s in the BUCKETSPIKE-1 spike, so
/// the interpreter thread must not block on it unboundedly.
///
/// Unlike `run_with_wall_clock_limit`'s killed subprocess, this resolve
/// thread has no forcible-cancellation primitive (it's plain Rust code, not
/// a child process) — on timeout it is abandoned (left to finish or fail on
/// its own in the background) rather than killed; only the *caller's* wait
/// is bounded. Same limitation `HostCap::call_with_deadline`'s own doc
/// comment describes for any non-subprocess blocking call.
pub(crate) fn resolve_with_deadline(
    specs: Vec<String>,
    deadline_ms: u64,
) -> Result<ResolvedEnvironment, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let config = Config::new();
        let index = Index::builtin();
        let result = resolve_multi(&specs, &config, &index).map_err(|e| e.to_string());
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_millis(deadline_ms.max(1))) {
        Ok(result) => result,
        Err(_) => Err(format!(
            "buckets provisioning did not complete within {deadline_ms}ms"
        )),
    }
}

/// Build a bwrap-sandboxed `std::process::Command` for `binary`/`exec_flag
/// code_str`, provisioning `binary` (+ `deps` — additional bare-runtime
/// buckets specs, see `Statement::LangBlock::deps`) via buckets first.
///
/// Returns the built command plus how many of `budget_ms` provisioning
/// consumed, so the caller can bound the actual sandboxed run with what's
/// left (`run_with_wall_clock_limit(cmd, remaining_ms)`).
pub(crate) fn build_sandboxed_command(
    lang: &str,
    binary: &'static str,
    exec_flag: &'static str,
    code_str: &str,
    deps: &[String],
    env_vars: &[(String, String)],
    budget_ms: u64,
) -> Result<(std::process::Command, u64), String> {
    let bucket_spec = lang_to_bucket_spec(lang).unwrap_or(binary).to_string();
    let mut specs = vec![bucket_spec];
    specs.extend(deps.iter().cloned());

    let t0 = Instant::now();
    let resolved = resolve_with_deadline(specs, budget_ms)?;
    let elapsed_ms = t0.elapsed().as_millis() as u64;

    let cwd: PathBuf = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    let profile = SandboxProfile {
        // Mirrors buckets' own `cmd_run`: the invocation cwd must be
        // rw-bound for `--chdir` to succeed inside bwrap's fresh mount
        // namespace (see CRUSHAST-BUCKETSPIKE-1's `SPIKE_RESULTS.md`).
        project_dir: Some(cwd.clone()),
        extra_ro_binds: resolved.installations.iter().map(|i| i.path.clone()).collect(),
        // No PyPI/npm dependency install (CRUSH-20 non-goal) needs no
        // network; keep the sandbox network-isolated by default.
        allow_network: false,
        ..Default::default()
    };

    let mut env: HashMap<String, String> = resolved.env.clone();
    for (name, val) in env_vars {
        env.insert(name.clone(), val.clone());
    }

    let args = vec![exec_flag.to_string(), code_str.to_string()];
    let cmd = sandboxed_command(binary, &args, &cwd, &env, &profile);
    let remaining_ms = budget_ms.saturating_sub(elapsed_ms).max(1);
    Ok((cmd, remaining_ms))
}
