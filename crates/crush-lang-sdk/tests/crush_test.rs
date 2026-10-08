//! Integration tests for the `crush` umbrella binary — a thin dispatcher
//! over `crush-repl` / `crush-run` / `crushc`. These tests only check that
//! dispatch reaches the right sibling tool with the right args; the
//! underlying tools' own test suites cover their actual behavior.

use std::process::Command;

fn crush_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush").unwrap_or("crush")
}

fn run_crush(args: &[&str]) -> std::process::Output {
    Command::new(crush_bin())
        .args(args)
        .output()
        .expect("failed to execute crush")
}

#[test]
fn crush_run_dispatches_to_crush_run() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.crush");
    std::fs::write(&src, "fn main() { io.print(\"hello\") }").unwrap();

    let output = run_crush(&["run", src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
}

#[test]
fn crush_build_dispatches_to_crushc() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.crush");
    let out = dir.path().join("hello.cvm1");
    std::fs::write(&src, "fn main() { io.print(\"hello\") }").unwrap();

    let output = run_crush(&["build", src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.exists(), "expected crushc's default output file");
}

#[test]
fn crush_unknown_subcommand_fails_with_usage() {
    let output = run_crush(&["bogus"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown subcommand"));
    assert!(stderr.contains("USAGE"));
}

#[test]
fn crush_help_prints_usage_and_succeeds() {
    let output = run_crush(&["--help"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("crush run FILE"));
}

#[test]
fn crush_version_prints_and_succeeds() {
    let output = run_crush(&["--version"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("crush "));
}

#[cfg(unix)]
fn fake_tool(dir: &std::path::Path, name: &str, version: &str) {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn crush_doctor_with_path(path: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(crush_bin())
        .arg("doctor")
        .args(args)
        .env("PATH", path)
        .output()
        .expect("failed to execute crush doctor")
}

#[cfg(unix)]
#[test]
fn crush_doctor_reports_runtimes_found_on_path() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(dir.path(), "python3", "Python 3.99.1");
    fake_tool(dir.path(), "node", "v99.0.0");
    fake_tool(dir.path(), "bash", "GNU bash, version 9.9.9");

    let output = crush_doctor_with_path(dir.path(), &["--json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout).expect(&stdout);
    let tool = |bin: &str| {
        report["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["binary"] == bin)
            .unwrap()
            .clone()
    };
    assert_eq!(tool("python3")["version"], "Python 3.99.1");
    assert_eq!(tool("node")["version"], "v99.0.0");
    assert_eq!(tool("bash")["version"], "GNU bash, version 9.9.9");
    assert_eq!(
        tool("node")["path"],
        dir.path().join("node").to_str().unwrap()
    );
    assert!(tool("buckets")["path"].is_null());
    assert_eq!(tool("buckets")["required"], false);
    // This test build has no sandboxed-polyglot, so bwrap is optional and the report passes.
    assert_eq!(report["features"]["sandboxed_polyglot"], false);
    assert_eq!(report["ok"], true);
    assert!(output.status.success(), "{stdout}");
}

#[cfg(unix)]
#[test]
fn crush_doctor_fails_when_a_polyglot_runtime_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(dir.path(), "python3", "Python 3.99.1");
    fake_tool(dir.path(), "bash", "GNU bash, version 9.9.9");

    let output = crush_doctor_with_path(dir.path(), &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("MISSING") && stdout.contains("node"),
        "{stdout}"
    );
    assert!(stdout.contains("Python 3.99.1"), "{stdout}");
}

#[test]
fn crush_doctor_rejects_unknown_arguments() {
    let output = run_crush(&["doctor", "--frobnicate"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown argument"));
}
