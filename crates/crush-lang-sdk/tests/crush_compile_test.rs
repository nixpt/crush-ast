//! Integration tests for the `crush-compile` binary and its
//! `--message-format json` diagnostic mode.
//!
//! Mirrors the existing `crushc_test.rs` and `crush_run_test.rs` shape.
//! Assembler failures route through `JsonDiagnostic::assembler_error`
//! (code `"E-ASM"`, file populated, `AssemblyError`'s `"line N: ..."`
//! display carried in `message`); true I/O failures stay on
//! `JsonDiagnostic::generic_error` with code `"E-IO"`.

use std::process::Command;

fn crush_compile_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-compile").unwrap_or("crush-compile")
}

fn run_crush_compile(args: &[&str]) -> std::process::Output {
    Command::new(crush_compile_bin())
        .args(args)
        .output()
        .expect("failed to execute crush-compile")
}

#[test]
fn crush_compile_emits_json_diagnostic_for_io_fallback() {
    // Missing input file produces a `std::io::Error` via `?`-propagation
    // in `compile`. The dispatch path falls back to
    // `JsonDiagnostic::generic_error` with `CODE_IO` so editors still see
    // a uniform NDJSON stream.
    let output = run_crush_compile(&[
        "--message-format",
        "json",
        "/nonexistent/crush-ast-path/missing.casm",
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
    let msg = parsed["message"].as_str().unwrap_or("");
    assert!(
        !msg.is_empty(),
        "expected non-empty underlying error text, got: {msg}"
    );
}

#[test]
fn crush_compile_emits_json_diagnostic_for_assembler_error() {
    // Bad CASM text causes `crush_lang_sdk::assemble` to return a
    // `crush_vm::AssemblyError`. The downcast in `crush-compile::main()`
    // routes that to `JsonDiagnostic::assembler_error` with code
    // `"E-ASM"`, the input file attached, and the underlying message
    // carrying the source line number (e.g. `line 1: ...`).
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.casm");
    std::fs::write(&src, "this is not valid CASM text\n").unwrap();
    let src_str = src.to_str().unwrap().to_string();

    let output = run_crush_compile(&[
        "--message-format",
        "json",
        &src_str,
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
    assert_eq!(parsed["code"].as_str(), Some("E-ASM"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert_eq!(parsed["file"].as_str(), Some(src_str.as_str()));
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
    let msg = parsed["message"].as_str().unwrap_or("");
    assert!(
        !msg.is_empty(),
        "expected non-empty assembler error text, got: {msg}"
    );
    assert!(
        msg.starts_with("line "),
        "expected `AssemblyError`'s `line N:` display prefix, got: {msg}"
    );
}

#[test]
fn crush_compile_default_message_format_remains_text() {
    // Default text mode preserves the `crush-compile:` prefix so users
    // who don't pass `--message-format` see the same plain-text errors
    // as before this PR.
    let output = run_crush_compile(&["/nonexistent/crush-ast-path/missing.casm"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim_start().starts_with('{'),
        "default mode unexpectedly emitted JSON: {stderr}"
    );
    assert!(
        stderr.contains("crush-compile"),
        "default text mode should keep `crush-compile:` prefix, got: {stderr}"
    );
}

#[test]
fn crush_compile_happy_path_json_mode_emits_no_diagnostic() {
    // A successful compile under `--message-format json` must NOT emit any
    // NDJSON diagnostic records to stderr (success is stdout-only — the
    // `Compiled X -> Y (N bytes)` line). This guards a regression where
    // a future contributor moves an `eprintln!` out of the `?`-chain and
    // accidentally surfaces a stray `{"code":"E-IO", ...}` record on
    // successful runs.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.casm");
    std::fs::write(&src, ".func main\nPUSH_STR \"hi\"\nHALT\n").unwrap();
    let out = dir.path().join("hello.cvm1");

    let output = run_crush_compile(&[
        "--message-format",
        "json",
        src.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "expected happy-path exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().starts_with('{'),
        "happy-path JSON mode accidentally leaked NDJSON record to stderr: {stderr}"
    );
    assert!(
        std::path::Path::new(&out).exists(),
        "expected compiled CVM1 blob at {}",
        out.display()
    );
}
