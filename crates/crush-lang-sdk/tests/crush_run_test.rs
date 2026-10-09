//! Integration tests for the `crush-run` binary and its
//! `--message-format json` diagnostic mode.

use std::process::Command;

fn crush_run_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}

fn run_crush_run(args: &[&str]) -> std::process::Output {
    Command::new(crush_run_bin())
        .args(args)
        .output()
        .expect("failed to execute crush-run")
}

#[test]
fn crush_run_emits_json_diagnostic_for_vm_runtime_error() {
    // A source with an infinite loop + a tight step quota produces a
    // RuntimeError::Vm(VmError::StepQuota(..)) which surfaces as
    // {code: "E-RT05", message: "", hint: "instruction quota exceeded..."}.
    // Because the variant uses `#[error(transparent)]` the inner VmError
    // Display equals `RuntimeError::Display`, so we put the full text in
    // `hint` and leave `message` empty to avoid editor-side redundancy.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("loop.crush");
    std::fs::write(
        &src,
        "fn main() { while true { let x = 1 } }\n",
    )
    .unwrap();

    let output = run_crush_run(&[
        "run",
        "--message-format",
        "json",
        src.to_str().unwrap(),
        "--max-steps",
        "5",
    ]);
    assert!(
        !output.status.success(),
        "expected non-zero exit on step-quota violation"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-RT05"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(
        parsed["file"].is_null(),
        "RuntimeError::Vm carries no source file path"
    );
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
    // `message` is empty to avoid duplicating `hint` (the Vm variant uses
    // `#[error(transparent)]`, so Display is identical).
    assert_eq!(
        parsed["message"].as_str().unwrap_or(""),
        "",
        "expected empty message on the Vm arm to avoid duplicating the VmError text"
    );
    let hint = parsed["hint"].as_str().unwrap_or("");
    assert!(
        hint.contains("instruction quota"),
        "expected hint to carry the full VmError display, got: {hint}"
    );
    // Lockdown: message and hint must not be byte-identical. Without this
    // guardrail a future contributor "helpfully" restoring the duplication
    // (e.g. by reverting the Vm arm to message: err.to_string()) would
    // silently regress the editor-facing schema without breaking these
    // independent assertions above.
    assert_ne!(
        parsed["message"], parsed["hint"],
        "RuntimeError::Vm records must put the full VmError text in only one field (message OR hint, not both)"
    );
}

#[test]
fn crush_run_emits_json_diagnostic_for_io_fallback() {
    // A missing file produces a std::io::Error via the `?`-propagation in
    // `run_file`, not a typed RuntimeError. The dispatch path falls back
    // to JsonDiagnostic::generic_error with CODE_IO so editors still see a
    // uniform NDJSON stream.
    let output = run_crush_run(&[
        "run",
        "--message-format",
        "json",
        "/nonexistent/crush-ast-path/missing.crush",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-IO"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(parsed["file"].is_null());
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
}

#[test]
fn crush_run_default_message_format_remains_text() {
    // Default text mode preserves the themed `[runtime]` badge so users
    // who don't pass `--message-format` see the same chain-walked output
    // as before this PR.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("loop.crush");
    std::fs::write(
        &src,
        "fn main() { while true { let x = 1 } }\n",
    )
    .unwrap();

    let output = run_crush_run(&[
        "run",
        src.to_str().unwrap(),
        "--max-steps",
        "5",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim_start().starts_with('{'),
        "default mode unexpectedly emitted JSON: {stderr}"
    );
    assert!(
        stderr.contains("[runtime]") || stderr.contains("runtime"),
        "default text mode should keep runtime-themed error, got: {stderr}"
    );
}

#[test]
fn crush_run_emits_rt01_for_invalid_cvm1_blob() {
    // `.cvm1` path now routes through `Runtime::run_blob`, which maps
    // `Program::from_blob` failures (bad magic, unsupported version,
    // truncated, bad manifest) to `RuntimeError::LoadBlob` → `E-RT01`
    // in JSON mode. Previously the binary called `Program::from_blob`
    // directly, so the bare `CrushError` fell past the downcast and
    // landed as the generic `E-IO` code — `E-RT01` was unreachable
    // from this CLI.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.cvm1");
    // 4 bytes ≠ the CVM1 magic header (`"CVM1"`) → `BadMagic` error.
    std::fs::write(&src, b"BADM\x00\x00\x00\x00").unwrap();

    let output = run_crush_run(&[
        "run",
        "--message-format",
        "json",
        src.to_str().unwrap(),
    ]);
    assert!(
        !output.status.success(),
        "expected non-zero exit on invalid CVM1 blob"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-RT01"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(parsed["file"].is_null());
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
    // `LoadBlob` carries no inner source right now (`Program::from_blob`'s
    // `CrushError` is collapsed into `RuntimeError::LoadBlob.to_string()`
    // rather than chained via `#[source]`). Pinning this contract catches
    // a future contributor who wires `Error::source()` into
    // `JsonDiagnostic::runtime_error` and silently leaks the underlying
    // `BadMagic`/`Truncated` text into both `message` and `hint`.
    assert!(
        parsed["hint"].is_null(),
        "E-RT01 records must not surface an inner-source hint; got: {}",
        parsed["hint"]
    );
    // `LoadBlob` keeps the underlying `BadMagic` text in `message`.
    assert!(
        !parsed["message"].as_str().unwrap_or("").is_empty(),
        "expected non-empty LoadBlob message, got: {}",
        parsed["message"]
    );
}

#[test]
fn crush_run_accepts_valid_cvm1_blob_for_regression() {
    // Happy-path regression guard for the `.cvm1` arm refactor: a real
    // CVM1 blob built via the public `crush_vm::assemble` + `to_blob`
    // API must still load *and* execute through `Runtime::run_blob`.
    // If quotas, host caps, or the load+execute wiring regressed, this
    // test fails before any other dispatch path would notice.
    let program = crush_vm::assemble(
        ".func main\nPUSH_STR \"hi\"\nCAP_CALL \"io.print\" 1\nHALT\n",
        Some(&["io.print"]),
        Some("hello"),
    )
    .expect("assemble should succeed");
    let blob = program.to_blob();

    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.cvm1");
    std::fs::write(&src, &blob).unwrap();

    let output = run_crush_run(&[
        "run",
        "--cap",
        "io.print",
        src.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "expected happy-path exit 0\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hi"),
        "expected stdout to contain 'hi' from io.print, got: {stdout}"
    );
}

#[test]
fn crush_run_reads_piped_stdin_through_source_pipeline() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("read.crush");
    std::fs::write(
        &src,
        "fn main() { let line = io.read(); io.print(line); return 0; }\n",
    )
    .unwrap();

    let mut child = Command::new(crush_run_bin())
        .args(["run", "--cap", "io.read", src.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute crush-run");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(b"piped input\n")
        .expect("write piped input");
    let output = child.wait_with_output().expect("wait for crush-run");

    assert!(
        output.status.success(),
        "io.read source pipeline failed\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout),
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "piped input\n");
}

// CRUSH-113: the stdlib is on by default — a conv.* call needs no flag —
// and `--no-stdlib` takes it away again.
#[test]
fn crush_run_registers_stdlib_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("conv.crush");
    std::fs::write(&src, "fn main() { io.print(conv.to_str(42)); return 0; }\n").unwrap();

    let output = run_crush_run(&["run", src.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(stdout.trim(), "42");

    // The old flag still parses and changes nothing.
    let output = run_crush_run(&["run", "--stdlib", src.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");

    let output = run_crush_run(&["run", "--no-stdlib", src.to_str().unwrap()]);
    assert!(
        !output.status.success(),
        "--no-stdlib must withhold conv.to_str"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("conv.to_str"), "stderr: {stderr}");
}

#[test]
fn crush_run_caps_lists_stdlib_as_default() {
    let output = run_crush_run(&["caps"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("on by default; --no-stdlib to disable"),
        "{stdout}"
    );
}

// CRUSH-151: the fs coreutils run end to end under `--fs`, `fs.cd` stays
// inside `--fs-root`, and without the grant they do not exist.
#[test]
fn crush_run_fs_coreutils_stay_in_the_sandbox() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        path.to_str().unwrap().to_string()
    };
    let ok = write(
        "ok.crush",
        r#"fs.mkdir("a/b", true)
fs.cd("a")
fs.touch("b/x.txt")
io.print(fs.pwd())
io.print(fs.find(".", "*.txt"))
fs.cd("..")
io.print(fs.pwd())
"#,
    );
    // crush-run drops stdout when the program fails, so the escape attempt
    // is its own program.
    let escape = write("escape.crush", "fs.cd(\"a\")\nfs.cd(\"../..\")\n");
    let fs_args = |src: &str| {
        let args = ["run", "--fs", "--fs-root", root.to_str().unwrap(), src];
        run_crush_run(&args)
    };

    let output = fs_args(&ok);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.starts_with("a\n[b/x.txt]\n.\n"), "stdout: {stdout}");
    assert!(root.join("a/b/x.txt").exists());

    let output = fs_args(&escape);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "cd above the root must fail");
    assert!(stderr.contains("escapes sandbox"), "stderr: {stderr}");

    // Without --fs the run is refused before anything executes (CRUSH-232).
    let output = run_crush_run(&["run", &ok]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("nothing was run"), "stderr: {stderr}");
    assert!(stderr.contains("--fs: ") && stderr.contains("fs.mkdir"), "stderr: {stderr}");
}

#[test]
fn crush_run_caps_lists_fs_coreutils() {
    let stdout = String::from_utf8_lossy(&run_crush_run(&["caps"]).stdout).into_owned();
    for cap in [
        "fs.ls", "fs.cat", "fs.pwd", "fs.cd", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "fs.touch",
        "fs.find",
    ] {
        assert!(
            stdout.contains(&format!("  {cap} ")),
            "{cap} missing from caps:\n{stdout}"
        );
    }
}

// CRUSH-155: `caps --json` lists every capability with its effects and grant.
#[test]
fn crush_run_caps_json_lists_effects_and_grants() {
    let output = run_crush_run(&["caps", "--json"]);
    assert!(output.status.success());
    let list: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let find = |name: &str| {
        list.iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .clone()
    };
    assert_eq!(find("fs.rm")["effects"], serde_json::json!(["fs/write"]));
    assert_eq!(find("fs.rm")["grant"], "--fs");
    assert_eq!(find("conv.to_str")["effects"], serde_json::json!([]));
    assert_eq!(find("io.print")["grant"], "portable");
    assert_eq!(find("time.sleep")["argc"], 1);
    assert!(
        list.iter().all(|c| c["effects"].is_array()),
        "every capability declares its effects"
    );
}

// ── CRUSH-232: declared capabilities, checked before running ───────────────

fn temp_program(body: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool.crush");
    std::fs::write(&path, body).unwrap();
    let path = path.to_str().unwrap().to_string();
    (dir, path)
}

const TOOL: &str = r#"@capabilities [fs.cat, time.now]
fn main() {
  io.print("starting")
  let t = time.now()
  io.print(str.len(fs.cat("tool.crush")))
  return 0
}
"#;

#[test]
fn crush_run_caps_file_shows_what_a_program_needs_without_running_it() {
    let (_dir, path) = temp_program(TOOL);
    let output = run_crush_run(&["caps", &path]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("declares: fs.cat, time.now"), "{stdout}");
    assert!(stdout.contains("--fs: fs.cat"), "{stdout}");
    assert!(stdout.contains("--time: time.now"), "{stdout}");
    assert!(stdout.contains("also uses (ambient): io.print"), "{stdout}");
    assert!(!stdout.contains("starting"), "caps must not run the program: {stdout}");

    let json = run_crush_run(&["caps", "--json", &path]);
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("json");
    assert_eq!(v["declared"], serde_json::json!(["fs.cat", "time.now"]));
    let fs_cat = v["uses"].as_array().unwrap().iter().find(|c| c["name"] == "fs.cat").unwrap();
    assert_eq!(fs_cat["grant"], "--fs");
    assert_eq!(fs_cat["ambient"], false);
}

#[test]
fn crush_run_refuses_ungranted_capabilities_before_running_anything() {
    let (dir, path) = temp_program(TOOL);
    let output = run_crush_run(&["run", &path]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(!stdout.contains("starting"), "nothing may run: {stdout}");
    assert!(stderr.contains("[capabilities]"), "{stderr}");
    assert!(stderr.contains("--fs: fs.cat") && stderr.contains("--time: time.now"), "{stderr}");

    // One grant short: still refused, naming only the missing one.
    let output = run_crush_run(&["run", "--fs", &path]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("--time: time.now") && !stderr.contains("fs.cat"), "{stderr}");

    let root = dir.path().to_str().unwrap();
    let output = run_crush_run(&["run", "--fs", "--fs-root", root, "--time", &path]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.starts_with("starting\n"), "{stdout}");
}

#[test]
fn crush_run_rejects_a_capability_the_program_did_not_declare() {
    let (_dir, path) = temp_program(
        "@capabilities [fs.cat]\nfn main() { io.print(env.get(\"HOME\")); return 0 }\n",
    );
    let output = run_crush_run(&["run", "--fs", "--env", &path]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("[compile]"), "{stderr}");
    assert!(stderr.contains("not declared in @capabilities: env.get"), "{stderr}");
}

#[test]
fn crush_run_empty_declaration_allows_only_ambient_capabilities() {
    let (_dir, pure) = temp_program("@capabilities []\nfn main() { io.print(str.len(\"abc\")); return 0 }\n");
    let output = run_crush_run(&["run", &pure]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("3\n"));

    let (_dir, poly) = temp_program("@capabilities []\nfn main() { @python { x = 1 } return 0 }\n");
    let output = run_crush_run(&["run", "--polyglot", &poly]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not declared in @capabilities: polyglot.python"), "{stderr}");
}

// ── Scripting basics: arguments, exit status, output that survives errors ───

#[test]
fn crush_run_passes_script_arguments_to_sys_args() {
    let (_dir, path) = temp_program(
        "fn main() {\n  let a = sys.args()\n  io.print(len(a))\n  io.print(a[1])\n  io.print(a[2])\n  return 0\n}\n",
    );
    // Flags may follow the arguments; `--` passes one that starts with `-`.
    let output = run_crush_run(&["run", &path, "one", "two words", "--max-steps", "1000", "--", "--three"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\ntwo words\n--three\n");
}

#[test]
fn crush_run_exits_with_the_status_given_to_sys_exit() {
    let (_dir, path) = temp_program(
        "fn main() {\n  io.print(\"before\")\n  sys.exit(3)\n  io.print(\"after\")\n  return 0\n}\n",
    );
    let output = run_crush_run(&["run", &path]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("[runtime]"));

    let (_dir, bad) = temp_program("fn main() { sys.exit(300) }\n");
    let output = run_crush_run(&["run", &bad]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("from 0 to 255"));
}

#[test]
fn crush_run_keeps_output_printed_before_a_runtime_error() {
    let (dir, path) = temp_program(
        "fn main() {\n  io.print(\"step 1\")\n  io.print(\"step 2\")\n  let x = fs.cat(\"missing.txt\")\n  return 0\n}\n",
    );
    let root = dir.path().to_str().unwrap();
    let output = run_crush_run(&["run", "--fs", "--fs-root", root, &path]);
    assert!(!output.status.success());
    // It used to print nothing: output was only written once the program ended.
    assert_eq!(String::from_utf8_lossy(&output.stdout), "step 1\nstep 2\n");
}

#[test]
fn crush_run_streams_output_while_the_program_runs() {
    use std::io::{BufRead, BufReader};
    let (_dir, path) = temp_program(
        "fn main() {\n  io.print(\"first\")\n  time.sleep(1500)\n  io.print(\"second\")\n  return 0\n}\n",
    );
    let mut child = Command::new(crush_run_bin())
        .args(["run", "--time", &path])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn crush-run");
    let start = std::time::Instant::now();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert_eq!(lines.next().unwrap().unwrap(), "first");
    let first_at = start.elapsed();
    assert_eq!(lines.next().unwrap().unwrap(), "second");
    child.wait().unwrap();
    assert!(
        first_at < std::time::Duration::from_millis(1200),
        "the first line arrived after {first_at:?}, i.e. only when the program ended"
    );
}

#[test]
fn crush_run_fs_write_as_a_statement_and_as_a_value() {
    let (dir, path) = temp_program(
        "fn main() {\n  fs.write(\"a.txt\", \"one\")\n  let r = fs.write(\"b.txt\", \"two\")\n  io.print(r)\n  io.print(fs.cat(\"a.txt\") + fs.cat(\"b.txt\"))\n  return 0\n}\n",
    );
    let root = dir.path().to_str().unwrap();
    let output = run_crush_run(&["run", "--fs", "--fs-root", root, &path]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "null\nonetwo\n");
}
