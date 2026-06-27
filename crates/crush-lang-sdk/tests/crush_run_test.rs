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
