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
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(out.exists(), "expected default output file to be created");
}

#[test]
fn crushc_check_valid_program() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ok.crush");
    std::fs::write(&src, "fn main() { io.print(\"ok\") }").unwrap();

    let output = run_crushc(&["--check", src.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
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
    assert!(stderr.contains("type error"), "stderr: {stderr}");
}

#[test]
fn crushc_emits_casm() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("emit.crush");
    std::fs::write(&src, "fn main() { io.print(\"hi\") }").unwrap();

    let output = run_crushc(&["--emit", "casm", src.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
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
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    assert!(out.exists());

    let run = Command::new(crush_run_bin())
        .args(&["run", out.to_str().unwrap()])
        .output()
        .expect("failed to execute crush-run");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("compiled"), "stdout: {stdout}");
}
