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
/// provision the language ITSELF. Sibling allowlist to
/// `scheduler::resolve_lang_binary`. This always resolves to a plain
/// `<project>@<constraint>` bottle spec (e.g. `python@3.11`) — the LANGUAGE
/// runtime only. Additional `@lang[...]` dependencies (bare bottle aliases
/// OR `pypi:`/`npm:` registry specs — CRUSH-66) are passed separately via
/// the caller's `deps` and validated by `validate_deps`, not mapped here.
/// Version constraints are loose (`^`) to track buckets' own resolvable
/// range, matching the CRUSHAST-BUCKETSPIKE-1/2 spike specs.
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
/// code_str`, provisioning `binary` plus `deps` via buckets first. `deps`
/// are OPAQUE buckets package specs (see `Statement::LangBlock::deps`):
/// bare bottle aliases (`openssl@^1.1`) or `pypi:`/`npm:` registry specs
/// (`pypi:six`, `npm:is-number@7`) — CRUSH-66. They are validated by
/// `validate_deps` (empty entries and scoped npm are rejected up front) and
/// otherwise handed to `resolve_multi` unchanged; `compose_env`'s
/// `PYTHONPATH`/`NODE_PATH` flow into the guest so the resolved packages are
/// importable WITHOUT giving the sandbox network access.
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
    validate_deps(deps)?;

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
        // Registry deps (`pypi:`/`npm:`) are resolved HOST-side by
        // `resolve_multi` above and RO-bound into the guest via
        // `extra_ro_binds` + `PYTHONPATH`/`NODE_PATH` — the guest itself
        // never installs anything, so it needs no network. Keep the sandbox
        // network-isolated by default (CRUSH-20 / CRUSH-66).
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

/// Validate a `@lang[...]` dependency spec list before handing it to
/// `buckets::resolve_multi` (CRUSH-66). Deps are opaque buckets specs —
/// bare bottle aliases (`openssl@^1.1`) or registry specs (`pypi:six`,
/// `npm:is-number@7`) — but two shapes are rejected up front so the failure
/// surfaces as a clear `SandboxSetup` diagnostic (CRUSH-18's phase model)
/// rather than a confusing resolve error deep in buckets:
///
/// - empty entries (a stray `@python[,]` or blank element), and
/// - scoped npm packages (`npm:@scope/name`): BUCKETS-15 v1 resolves
///   UNSCOPED registry names only, so a scoped spec can never succeed —
///   reject it with a message that names the limitation instead of letting
///   it fail opaquely at inventory time.
///
/// Everything else (including genuinely unknown packages) is left to
/// `resolve_multi`, whose own error is mapped to `SandboxSetup` by the
/// caller. A leading `@` after the `npm:` scheme marks a scope; an `@`
/// elsewhere (`npm:is-number@7`) is a version pin and is allowed.
pub(crate) fn validate_deps(deps: &[String]) -> Result<(), String> {
    for dep in deps {
        if dep.trim().is_empty() {
            return Err("empty dependency spec in @lang[...] list".to_string());
        }
        if let Some(rest) = dep.strip_prefix("npm:") {
            if rest.starts_with('@') {
                return Err(format!(
                    "scoped npm package {dep:?} is not supported by buckets v1 \
                     (unscoped registry names only); use an unscoped package or \
                     install it into the host cellar manually"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_string()
    }

    #[test]
    fn validate_deps_accepts_bare_and_registry_specs() {
        assert!(validate_deps(&[
            s("openssl@^1.1"),
            s("pypi:six"),
            s("npm:is-number@7"),
            s("cargo:ripgrep"),
        ])
        .is_ok());
    }

    #[test]
    fn validate_deps_accepts_empty_list() {
        let deps: Vec<String> = Vec::new();
        assert!(validate_deps(&deps).is_ok());
    }

    #[test]
    fn validate_deps_rejects_empty_entry() {
        let err = validate_deps(&[s("pypi:six"), s("  ")]).unwrap_err();
        assert!(err.contains("empty dependency"), "{err}");
    }

    #[test]
    fn validate_deps_rejects_scoped_npm() {
        let err = validate_deps(&[s("npm:@types/node")]).unwrap_err();
        assert!(err.contains("scoped npm"), "{err}");
        assert!(err.contains("npm:@types/node"), "{err}");
    }

    #[test]
    fn validate_deps_allows_unscoped_npm_with_version_pin() {
        // `npm:is-number@7` has an `@` but it is a version pin, not a scope —
        // only a LEADING `@` after `npm:` marks a scoped package.
        assert!(validate_deps(&[s("npm:is-number@7")]).is_ok());
    }
}
