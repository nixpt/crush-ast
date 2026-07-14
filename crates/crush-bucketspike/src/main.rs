//! CRUSHAST-BUCKETSPIKE-1/2 — throwaway empirical spike, NOT production code.
//!
//! BUCKETSPIKE-1 (the python run below, same source/logic, now sharing the
//! generalized `run_language` harness with node/bash) proved: can a
//! `@python { ... }` polyglot block run inside a `buckets`-provisioned,
//! bwrap-sandboxed `python3` instead of the host's `python3`, with the
//! existing sentinel-line JSON marshaling protocol
//! (`crush_vm::scheduler::EXEC_LANG` + `crush-lang-sdk`'s
//! `rewrite_python_marshaling`) surviving unchanged? Answer: yes.
//!
//! BUCKETSPIKE-2 extends the question to the other two languages
//! `crush-vm::scheduler::resolve_lang_binary` maps to a host subprocess
//! today: `node -e <code>` (for `@javascript`) and `bash -c <code>` (for
//! `@bash`). Node and bash do NOT have a real marshaling protocol the way
//! Python does (see `crates/crush-lang-sdk/README.md`'s "Polyglot blocks"
//! section: "Only Python has this") — that's explicitly out of scope here.
//! This spike is purely about the SANDBOXING question: does
//! `buckets::sandbox::sandboxed_command` + piped stdio work the same way
//! for `node -e` and `bash -c` as it did for `python3 -c`, and what does
//! cold/warm provisioning cost for each (different bottles, different
//! sizes — not assumed to match Python's numbers).
//!
//! None of this touches the real EXEC_LANG opcode handler. See
//! SPIKE_RESULTS.md (BUCKETSPIKE-1) and SPIKE_RESULTS_2.md
//! (BUCKETSPIKE-2) at the repo root for the real captured numbers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use buckets::config::Config;
use buckets::index::Index;
use buckets::resolve::resolve_multi;
use buckets::sandbox::{sandboxed_command, SandboxProfile};
use buckets::types::ResolvedEnvironment;

use crush_vm::scheduler::CRUSH_RESULT_SENTINEL;
use crush_vm::vm::Value;

/// Replicates `crates/crush-vm/src/scheduler.rs`'s EXEC_LANG opcode
/// handler's stdout-scanning logic exactly: split on lines, pull out the
/// sentinel-prefixed line (last one wins), everything else is "visible"
/// program output (joined + trimmed), and the sentinel payload is
/// JSON-decoded into a `crush_vm::vm::Value` — falling back to
/// `Value::Str` on decode failure, same as the real handler.
///
/// Shared verbatim across python/node/bash below: the whole point of
/// BUCKETSPIKE-2 is proving this same mechanism generalizes across
/// languages, not just Python's real marshaling protocol.
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
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join("bwrap").is_file()))
        .unwrap_or(false)
}

/// Resolve `spec` via buckets and run `program -flag code` sandboxed,
/// returning (stdout, stderr, success, resolve_duration). Generic over
/// language: BUCKETSPIKE-1's python-specific version has been generalized
/// here so node/bash reuse the exact same resolve+sandbox+capture path
/// (only the buckets spec, program/flag/code, and extra env vary).
fn resolve_and_run(
    spec: &str,
    program: &str,
    flag: &str,
    code: &str,
    extra_env: &HashMap<String, String>,
    cache_dir: &PathBuf,
    index: &Index,
) -> anyhow::Result<(String, String, bool, Duration)> {
    std::env::set_var("BUCKETS_CACHE_DIR", cache_dir);
    // IMPORTANT (found live, the hard way): `Config::new()` reads
    // `BUCKETS_CACHE_DIR` ONCE at construction and stores it in a struct
    // field (see buckets/src/config.rs) — calling `set_var` on an
    // already-built `Config` is a silent no-op. Building `Config` here,
    // strictly AFTER `set_var` above, is what actually makes each
    // cold/warm pair below use its own fresh per-language cache dir
    // instead of all three languages quietly sharing the real default
    // `~/.buckets` (which already had leftover state from prior spike
    // runs — first version of this harness built `Config` once in
    // `main()` before any `set_var`, and every "cold" run came back in
    // single-digit milliseconds because it was actually a cache hit
    // against real leftover state, not a fresh provision).
    let config = Config::new();

    let t0 = Instant::now();
    let resolved: ResolvedEnvironment = resolve_multi(&[spec.to_string()], &config, index)?;
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
    for (k, v) in extra_env {
        env.insert(k.clone(), v.clone());
    }

    let args = vec![flag.to_string(), code.to_string()];
    let mut cmd = sandboxed_command(program, &args, &cwd, &env, &profile);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    // No stdin needed for these snippets, but close it explicitly to prove
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

struct LangResult {
    label: &'static str,
    cold_ok: bool,
    warm_ok: bool,
    cold_resolve_dur: Duration,
    cold_wall: Duration,
    warm_resolve_dur: Duration,
    warm_wall: Duration,
    cold_stdout: String,
    warm_stdout: String,
    cold_visible: String,
    warm_visible: String,
    cold_value: Value,
    warm_value: Value,
    checks_ok: bool,
}

/// Run the cold-then-warm pair for one language, printing progress exactly
/// like BUCKETSPIKE-1 did, and return the captured numbers for the
/// SPIKE_RESULTS_2.md addendum / final summary table.
#[allow(clippy::too_many_arguments)]
fn run_language(
    label: &'static str,
    spec: &str,
    program: &str,
    flag: &str,
    code: &str,
    extra_env: &HashMap<String, String>,
    expect_visible: &str,
    expect_value: i64,
    index: &Index,
) -> anyhow::Result<LangResult> {
    println!("\n=== {label} ===");

    // Fresh, per-language BUCKETS_CACHE_DIR so cold is a REAL cold
    // provision (no pre-existing cache from a prior language or a prior
    // run of this same spike short-circuiting it). Deliberately NOT
    // shared with python's cache dir or between node/bash, per the task
    // brief's "don't assume they match Python's numbers — different
    // bottles, different sizes."
    let cache_dir: PathBuf = std::env::temp_dir().join(format!(
        "bucketspike2-cache-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&cache_dir)?;
    println!("fresh BUCKETS_CACHE_DIR: {}", cache_dir.display());

    println!("--- COLD run (fresh cache, first resolve+install+sandbox+exec) ---");
    let cold_wall_t0 = Instant::now();
    let (cold_stdout, cold_stderr, cold_ok, cold_resolve_dur) =
        resolve_and_run(spec, program, flag, code, extra_env, &cache_dir, index)?;
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

    println!("--- WARM run (same cache dir, second resolve = cache hit) ---");
    let warm_wall_t0 = Instant::now();
    let (warm_stdout, warm_stderr, warm_ok, warm_resolve_dur) =
        resolve_and_run(spec, program, flag, code, extra_env, &cache_dir, index)?;
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

    let mut checks_ok = true;
    if !cold_ok {
        eprintln!("FAIL[{label}]: cold run did not exit successfully");
        checks_ok = false;
    }
    if !warm_ok {
        eprintln!("FAIL[{label}]: warm run did not exit successfully");
        checks_ok = false;
    }
    if cold_visible != expect_visible {
        eprintln!("FAIL[{label}]: cold visible output mismatch, got {:?}", cold_visible);
        checks_ok = false;
    }
    if warm_visible != expect_visible {
        eprintln!("FAIL[{label}]: warm visible output mismatch, got {:?}", warm_visible);
        checks_ok = false;
    }
    if !matches!(cold_value, Value::Int(n) if n == expect_value) {
        eprintln!("FAIL[{label}]: cold decoded sentinel value mismatch, got {:?}", cold_value);
        checks_ok = false;
    }
    if !matches!(warm_value, Value::Int(n) if n == expect_value) {
        eprintln!("FAIL[{label}]: warm decoded sentinel value mismatch, got {:?}", warm_value);
        checks_ok = false;
    }

    println!(
        "[{label}] cold resolve_duration: {cold_resolve_dur:?}  (total incl. sandbox exec: {cold_wall:?})"
    );
    println!(
        "[{label}] warm resolve_duration: {warm_resolve_dur:?}  (total incl. sandbox exec: {warm_wall:?})"
    );
    println!("[{label}] proof: {}", if checks_ok { "PASS" } else { "FAIL" });

    Ok(LangResult {
        label,
        cold_ok,
        warm_ok,
        cold_resolve_dur,
        cold_wall,
        warm_resolve_dur,
        warm_wall,
        cold_stdout,
        warm_stdout,
        cold_visible,
        warm_visible,
        cold_value,
        warm_value,
        checks_ok,
    })
}

fn main() -> anyhow::Result<()> {
    println!("=== CRUSHAST-BUCKETSPIKE-2 ===");
    println!("(node + bash, extending BUCKETSPIKE-1's python-only result)");

    let bwrap_present = bwrap_on_path();
    println!("bwrap on PATH: {bwrap_present}");
    if !bwrap_present {
        println!(
            "WARNING: bwrap not found — buckets::sandbox::sandboxed_command will fall back to \
             an UNSANDBOXED Command with a stderr warning. Any latency/marshaling numbers below \
             would NOT be exercising the real bwrap sandbox path."
        );
    }

    let index = Index::builtin();

    // ---- PYTHON (BUCKETSPIKE-1, re-run here unchanged in shape) ----
    // The exact source `rewrite_python_marshaling` would generate for:
    // `fn main() { let base = 5; @python { print("hello from sandboxed
    // python"); result = base + 1 } print(result); }` — same literal source
    // as BUCKETSPIKE-1's spike, now run through the generalized
    // `run_language`/`resolve_and_run` harness that node/bash also use, as
    // a regression check that generalizing the harness didn't change
    // python's own already-proven result. See SPIKE_RESULTS.md for the
    // original BUCKETSPIKE-1 numbers this is compared against.
    let python_code = r#"import json as __crush_json, os as __crush_os; base = __crush_json.loads(__crush_os.environ["base"])
print("hello from sandboxed python")
result = base + 1
try:
    print("\x00CRUSH_RESULT\x00" + __crush_json.dumps(result))
except TypeError as __crush_marshal_err:
    import sys as __crush_sys
    print("cannot marshal output variable 'result' (type " + type(result).__name__ + "): " + str(__crush_marshal_err), file=__crush_sys.stderr)
    __crush_sys.exit(1)
"#;
    let mut python_env: HashMap<String, String> = HashMap::new();
    python_env.insert("base".to_string(), "5".to_string());
    let python_result = run_language(
        "python",
        "python@3.11",
        "python3",
        "-c",
        python_code,
        &python_env,
        "hello from sandboxed python",
        6,
        &index,
    )?;

    // ---- NODE ----
    // `console.log(...)` for ordinary visible output; a second line that's
    // structurally identical to the real sentinel line python's marshaling
    // epilogue emits (NUL + "CRUSH_RESULT" + NUL + JSON payload) — proving
    // the MECHANISM (piped stdio + a NUL-byte-bearing line surviving intact
    // through bwrap + Rust's Command capture) generalizes, NOT implementing
    // real @javascript marshaling (out of scope, no free-variable analysis
    // wired for JS — see crush-lang-sdk/README.md).
    let node_code = r#"console.log("hello from sandboxed node"); console.log("\x00CRUSH_RESULT\x00" + JSON.stringify(6));"#;
    let node_env: HashMap<String, String> = HashMap::new();
    let node_result = run_language(
        "node",
        "node@20",
        "node",
        "-e",
        node_code,
        &node_env,
        "hello from sandboxed node",
        6,
        &index,
    )?;

    // ---- BASH ----
    // Same mechanism-proof shape as node, plus (unlike node/python) a
    // two-line, no-force-required bonus: bash already exposes environment
    // variables as bare `$name`, no decode step needed the way Python's
    // marshaling prologue needs `json.loads(os.environ["base"])`. So this
    // also proves env-var passthrough survives bwrap by actually using an
    // env var (`base=5`) in computing the sentinel payload (`base + 1 = 6`),
    // which the eventual real marshaling protocol (if bash ever gets one)
    // would depend on.
    let bash_code = r#"echo "hello from sandboxed bash"; result=$((base + 1)); printf '\0CRUSH_RESULT\0%s\n' "$result""#;
    let mut bash_env: HashMap<String, String> = HashMap::new();
    bash_env.insert("base".to_string(), "5".to_string());
    let bash_result = run_language(
        "bash",
        "bash@5",
        "bash",
        "-c",
        bash_code,
        &bash_env,
        "hello from sandboxed bash",
        6,
        &index,
    )?;

    println!("\n=== SUMMARY (BUCKETSPIKE-1 python regression + BUCKETSPIKE-2 node/bash) ===");
    println!("bwrap on PATH: {bwrap_present}");
    for r in [&python_result, &node_result, &bash_result] {
        println!(
            "[{}] cold={:?} (resolve {:?}) warm={:?} (resolve {:?}) proof={}",
            r.label,
            r.cold_wall,
            r.cold_resolve_dur,
            r.warm_wall,
            r.warm_resolve_dur,
            if r.checks_ok { "PASS" } else { "FAIL" }
        );
    }

    let all_ok = bwrap_present && python_result.checks_ok && node_result.checks_ok && bash_result.checks_ok;
    println!("overall: {}", if all_ok { "PASS" } else { "FAIL" });

    // Silence unused-field warnings for fields only consumed via {:?} above
    // in the per-language printouts (kept on the struct for a future
    // consumer / for clarity when reading this file, e.g. raw stdout).
    for r in [&python_result, &node_result, &bash_result] {
        let _ = (
            &r.cold_ok,
            &r.warm_ok,
            &r.cold_stdout,
            &r.warm_stdout,
            &r.cold_visible,
            &r.warm_visible,
            &r.cold_value,
            &r.warm_value,
        );
    }

    if !all_ok {
        std::process::exit(1);
    }
    Ok(())
}
