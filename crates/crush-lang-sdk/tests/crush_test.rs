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
