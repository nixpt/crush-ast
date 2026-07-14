//! CRUSHAST-BUCKETSPIKE-1 — throwaway empirical spike, NOT production code.
//!
//! Question: can a `@python { ... }` polyglot block run inside a
//! `buckets`-provisioned, bwrap-sandboxed `python3` instead of the host's
//! `python3`, with the existing sentinel-line JSON marshaling protocol
//! (`crush_vm::scheduler::EXEC_LANG` + `crush-lang-sdk`'s
//! `rewrite_python_marshaling`) surviving unchanged?
//!
//! This binary does NOT touch the real EXEC_LANG opcode handler. It
//! manually replicates:
//!   1. the exact Python source shape `rewrite_python_marshaling` emits
//!      (prologue: JSON-decode inputs from env vars; the block's own
//!      `print(...)`; epilogue: sentinel-prefixed JSON-encoded result), and
//!   2. the exact stdout-scanning logic `scheduler.rs`'s EXEC_LANG handler
//!      uses (split lines, pull out the sentinel-prefixed line, everything
//!      else is "visible" program output, last sentinel line wins).
//!
//! ... but runs the subprocess via `buckets::sandbox::sandboxed_command`
//! (bwrap-sandboxed) instead of a bare `std::process::Command::new("python3")`,
//! and times cold vs warm provisioning via buckets.
//!
//! See SPIKE_RESULTS.md at the repo root for the real captured numbers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use buckets::config::Config;
use buckets::index::Index;
use buckets::resolve::resolve_multi;
use buckets::sandbox::{sandboxed_command, SandboxProfile};
use buckets::types::ResolvedEnvironment;

use crush_vm::scheduler::CRUSH_RESULT_SENTINEL;
use crush_vm::vm::Value;

/// The literal Python source `rewrite_python_marshaling` would generate
/// for: `fn main() { let base = 5; @python { print("hello from sandboxed
/// python"); result = base + 1 } print(result); }` — hand-copied to match
/// `crates/crush-lang-sdk/src/compile.rs::rewrite_python_marshaling`'s
/// prologue/epilogue shape byte-for-byte (including the literal
/// `\x00CRUSH_RESULT\x00` sentinel, which that function deliberately
/// hand-writes rather than deriving from Rust's `CRUSH_RESULT_SENTINEL`
/// via `{:?}` — see its doc comment for why the two escaping grammars
/// only coincidentally overlap).
const PYTHON_SOURCE: &str = r#"import json as __crush_json, os as __crush_os; base = __crush_json.loads(__crush_os.environ["base"])
print("hello from sandboxed python")
result = base + 1
try:
    print("\x00CRUSH_RESULT\x00" + __crush_json.dumps(result))
except TypeError as __crush_marshal_err:
    import sys as __crush_sys
    print("cannot marshal output variable 'result' (type " + type(result).__name__ + "): " + str(__crush_marshal_err), file=__crush_sys.stderr)
    __crush_sys.exit(1)
"#;

/// Replicates `crates/crush-vm/src/scheduler.rs`'s EXEC_LANG opcode
/// handler's stdout-scanning logic exactly: split on lines, pull out the
/// sentinel-prefixed line (last one wins), everything else is "visible"
/// program output (joined + trimmed), and the sentinel payload is
/// JSON-decoded into a `crush_vm::vm::Value` — falling back to
/// `Value::Str` on decode failure, same as the real handler.
fn scan_stdout(raw: &str) -> (String, Value) {
    let mut visible_lines: Vec<&str> = Vec::new();
    let mut result_payload: Option<&str> = None;
    for line in raw.lines() {
        match line.strip_prefix(CRUSH_RESULT_SENTINEL) {
            Some(payload) => result_payload = Some(payload),
            None => visible_lines.push(line),
        }
    }
    let visible = visible_lines.join("\n").trim().to_string();
    let result_value = match result_payload {
        Some(payload) => {
            serde_json::from_str::<Value>(payload).unwrap_or_else(|_| Value::Str(payload.to_string()))
        }
        None => Value::Str(visible.clone()),
    };
    (visible, result_value)
}

fn bwrap_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join("bwrap").is_file())
        })
        .unwrap_or(false)
}

/// Resolve `python@3.11` via buckets and run the sandboxed
/// `python3 -c <PYTHON_SOURCE>` command, returning (stdout, stderr,
/// success, resolve_duration).
fn resolve_and_run(config: &Config, index: &Index) -> anyhow::Result<(String, String, bool, std::time::Duration)> {
    let t0 = Instant::now();
    let resolved: ResolvedEnvironment = resolve_multi(&["python@3.11".to_string()], config, index)?;
    let resolve_duration = t0.elapsed();

    let cwd = std::env::current_dir()?;
    let profile = SandboxProfile {
        // Mirrors buckets' own `cmd_run`: the invocation cwd must be
        // rw-bound for `--chdir` to succeed inside bwrap's fresh mount
        // namespace.
        project_dir: Some(cwd.clone()),
        extra_ro_binds: resolved.installations.iter().map(|i| i.path.clone()).collect(),
        allow_network: false,
    };

    let mut env: HashMap<String, String> = resolved.env.clone();
    // The marshaled input: JSON-encoded, matching the real EXEC_LANG
    // handler's `cmd.env(name, val.as_text())` — `5` is valid JSON for an
    // int, exactly what `Value::Int(5)`'s Display/as_text produces.
    env.insert("base".to_string(), "5".to_string());

    let args = vec!["-c".to_string(), PYTHON_SOURCE.to_string()];
    let mut cmd = sandboxed_command("python3", &args, &cwd, &env, &profile);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    // No stdin needed for this snippet, but close it explicitly to prove
    // the piped-stdin path itself doesn't hang/break under bwrap.
    drop(child.stdin.take());
    let output = child.wait_with_output()?;

    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
        resolve_duration,
    ))
}

fn main() -> anyhow::Result<()> {
    println!("=== CRUSHAST-BUCKETSPIKE-1 ===");

    let bwrap_present = bwrap_on_path();
    println!("bwrap on PATH: {bwrap_present}");
    if !bwrap_present {
        println!(
            "WARNING: bwrap not found — buckets::sandbox::sandboxed_command will fall back to \
             an UNSANDBOXED Command with a stderr warning. Any latency/marshaling numbers below \
             would NOT be exercising the real bwrap sandbox path."
        );
    }

    // Fresh cache dir so the first resolve is a REAL cold provision (no
    // pre-existing ~/.buckets/python.org install to short-circuit it).
    let cache_dir: PathBuf = std::env::temp_dir().join(format!(
        "bucketspike-cache-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&cache_dir)?;
    println!("fresh BUCKETS_CACHE_DIR: {}", cache_dir.display());
    std::env::set_var("BUCKETS_CACHE_DIR", &cache_dir);

    let config = Config::new();
    let index = Index::builtin();

    // ---- COLD ----
    println!("\n--- COLD run (fresh cache, first resolve+install+sandbox+exec) ---");
    let cold_wall_t0 = Instant::now();
    let (cold_stdout, cold_stderr, cold_ok, cold_resolve_dur) = resolve_and_run(&config, &index)?;
    let cold_wall = cold_wall_t0.elapsed();
    println!("cold: success={cold_ok} resolve_duration={cold_resolve_dur:?} total_wall={cold_wall:?}");
    println!("cold stdout (raw, with embedded NUL sentinel bytes shown as \\0):");
    println!("{:?}", cold_stdout);
    if !cold_stderr.is_empty() {
        println!("cold stderr: {cold_stderr}");
    }

    let (cold_visible, cold_value) = scan_stdout(&cold_stdout);
    println!("cold visible output: {:?}", cold_visible);
    println!("cold decoded sentinel Value: {:?}", cold_value);

    // ---- WARM ----
    println!("\n--- WARM run (same cache dir, second resolve = cache hit) ---");
    let warm_wall_t0 = Instant::now();
    let (warm_stdout, warm_stderr, warm_ok, warm_resolve_dur) = resolve_and_run(&config, &index)?;
    let warm_wall = warm_wall_t0.elapsed();
    println!("warm: success={warm_ok} resolve_duration={warm_resolve_dur:?} total_wall={warm_wall:?}");
    println!("warm stdout (raw):");
    println!("{:?}", warm_stdout);
    if !warm_stderr.is_empty() {
        println!("warm stderr: {warm_stderr}");
    }

    let (warm_visible, warm_value) = scan_stdout(&warm_stdout);
    println!("warm visible output: {:?}", warm_visible);
    println!("warm decoded sentinel Value: {:?}", warm_value);

    // ---- Assertions (proof, not just printout) ----
    let mut all_ok = true;

    if !cold_ok {
        eprintln!("FAIL: cold run did not exit successfully");
        all_ok = false;
    }
    if !warm_ok {
        eprintln!("FAIL: warm run did not exit successfully");
        all_ok = false;
    }
    if cold_visible != "hello from sandboxed python" {
        eprintln!(
            "FAIL: cold visible output mismatch, got {:?}",
            cold_visible
        );
        all_ok = false;
    }
    if warm_visible != "hello from sandboxed python" {
        eprintln!(
            "FAIL: warm visible output mismatch, got {:?}",
            warm_visible
        );
        all_ok = false;
    }
    if !matches!(cold_value, Value::Int(6)) {
        eprintln!("FAIL: cold decoded sentinel value mismatch, got {:?}", cold_value);
        all_ok = false;
    }
    if !matches!(warm_value, Value::Int(6)) {
        eprintln!("FAIL: warm decoded sentinel value mismatch, got {:?}", warm_value);
        all_ok = false;
    }

    println!("\n=== SUMMARY ===");
    println!("bwrap exercised: {bwrap_present}");
    println!("cold resolve_duration: {cold_resolve_dur:?}  (total incl. sandbox exec: {cold_wall:?})");
    println!("warm resolve_duration: {warm_resolve_dur:?}  (total incl. sandbox exec: {warm_wall:?})");
    println!("marshaling proof: {}", if all_ok { "PASS" } else { "FAIL" });

    if !all_ok {
        std::process::exit(1);
    }
    Ok(())
}
