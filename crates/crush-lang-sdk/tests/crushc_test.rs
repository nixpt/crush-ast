//! Integration tests for the `crushc` command-line compiler.

use std::process::Command;

fn crushc_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crushc").unwrap_or("crushc")
}

fn crush_run_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}

fn run_crushc(args: &[&str]) -> std::process::Output {
    Command::new(crushc_bin())
        .args(args)
        .output()
        .expect("failed to execute crushc")
}

#[test]
fn crushc_compiles_to_cvm1_default_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.crush");
    let out = dir.path().join("hello.cvm1");
    std::fs::write(&src, "fn main() { io.print(\"hello\") }").unwrap();

    let output = run_crushc(&[src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.exists(), "expected default output file to be created");
}

#[test]
fn crushc_check_valid_program() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ok.crush");
    std::fs::write(&src, "fn main() { io.print(\"ok\") }").unwrap();

    let output = run_crushc(&["--check", src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no errors detected"), "stderr: {stderr}");
}

#[test]
fn crushc_reports_type_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    std::fs::write(&src, "fn main() { let x = true + 1 }").unwrap();

    let output = run_crushc(&[src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Post-readability pass the CLI uses a `[type]` badge for type-check
    // errors; the inner message still describes the underlying mismatch.
    assert!(
        stderr.contains("[type]") && stderr.contains("Invalid binary op +"),
        "stderr: {stderr}"
    );
}

#[test]
fn crushc_emits_json_diagnostic_for_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    // Unterminated curly — produces exactly one parse error.
    std::fs::write(&src, "fn main() {").unwrap();

    let output = run_crushc(&["--message-format", "json", src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // One NDJSON record on stderr.
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert!(parsed["code"].is_string(), "code: {}", parsed);
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(parsed["file"].is_string());
    assert!(parsed["line"].as_u64().is_some());
    assert!(parsed["col"].as_u64().is_some());
    assert!(parsed["message"].is_string());
    assert!(parsed["hint"].is_null() || parsed["hint"].is_string());
}

#[test]
fn crushc_emits_json_diagnostic_for_type_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    std::fs::write(&src, "fn main() { let x = true + 1 }").unwrap();

    let output = run_crushc(&["--message-format", "json", src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-TP01"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(parsed["file"].is_string());
    // Semantic errors don't yet carry source coordinates.
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
    assert!(parsed["message"].as_str().unwrap_or("").contains("binary"));
}

#[test]
fn crushc_default_message_format_remains_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    std::fs::write(&src, "fn main() { let x = true + 1 }").unwrap();

    let output = run_crushc(&[src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Default is text — must not emit raw JSON to stderr.
    assert!(
        !stderr.trim_start().starts_with('{'),
        "default mode unexpectedly emitted JSON: {stderr}"
    );
    assert!(stderr.contains("[type]"), "stderr: {stderr}");
}

#[test]
fn crushc_emits_json_diagnostic_for_io_error() {
    // --message-format json covers non-themed failures (file-not-found,
    // unknown emit kind, etc.) so editors get a uniform NDJSON stream.
    let output = run_crushc(&[
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
    // IO failures don't carry source coordinates.
    assert!(parsed["file"].is_null());
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
    assert!(
        parsed["message"]
            .as_str()
            .unwrap_or("")
            .contains("No such file")
            || parsed["message"]
                .as_str()
                .unwrap_or("")
                .contains("cannot read"),
        "expected human-readable IO message, got: {}",
        parsed["message"]
    );
}

#[test]
fn crushc_default_message_format_remains_text_for_io_error() {
    // Symmetry check: default text mode on a missing file still emits
    // a `crushc: ...` prefix line and not JSON.
    let output = run_crushc(&["/nonexistent/crush-ast-path/missing.crush"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim_start().starts_with('{'),
        "default mode unexpectedly emitted JSON: {stderr}"
    );
    assert!(
        stderr.contains("crushc:"),
        "expected `crushc:` prefix in default text mode, got: {stderr}"
    );
}

#[test]
fn crushc_emits_casm() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("emit.crush");
    std::fs::write(&src, "fn main() { io.print(\"hi\") }").unwrap();

    let output = run_crushc(&["--emit", "casm", src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(".func main"), "stdout: {stdout}");
    assert!(stdout.contains("io.print"), "stdout: {stdout}");
}

#[test]
fn crushc_compiled_program_runs() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("run.crush");
    let out = dir.path().join("run.cvm1");
    std::fs::write(&src, "fn main() { io.print(\"compiled\") }").unwrap();

    let compile = run_crushc(&["-o", out.to_str().unwrap(), src.to_str().unwrap()]);
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(out.exists());

    let run = Command::new(crush_run_bin())
        .args(["run", out.to_str().unwrap()])
        .output()
        .expect("failed to execute crush-run");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("compiled"), "stdout: {stdout}");
}
