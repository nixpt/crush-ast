//! CRUSH-241 (GitHub #386 / #370): a script's data stays on stdout.
//!
//! Three separate noises used to land on the stream a pipeline reads:
//! - `crush-run run` printed `[steps=N, stack=M]` on stderr for every run, with
//!   no way to turn it off;
//! - there was no `io.eprint`, so a script's diagnostics had to share stdout
//!   with its data (`script | next` could not separate them);
//! - a *registered* capability that failed was reported as
//!   `unknown capability: fs.cat: fs.cat x: …`, i.e. a working capability
//!   claiming not to exist.

use std::path::Path;
use std::process::{Command, Output, Stdio};

fn crush_run_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(crush_run_bin())
        .args(args)
        .current_dir(dir)
        // Never inherit stdin: an `io.read` would block the test forever.
        .stdin(Stdio::null())
        .env_remove("CRUSH_DEBUG")
        .output()
        .expect("spawn crush-run")
}

fn write_program(dir: &Path, name: &str, source: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path.to_str().unwrap().to_string()
}

/// `[steps=…]` is debugging telemetry, not program output: off by default, and
/// available with `--stats` or `CRUSH_DEBUG` for debugging (GitHub #386).
#[test]
fn steps_line_is_only_printed_when_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_program(dir.path(), "data.crush", "fn main() { io.print(\"data\") }");

    let quiet = run_in(dir.path(), &["run", &src]);
    assert!(quiet.status.success(), "stderr: {}", String::from_utf8_lossy(&quiet.stderr));
    assert_eq!(String::from_utf8_lossy(&quiet.stdout), "data\n");
    assert_eq!(
        String::from_utf8_lossy(&quiet.stderr),
        "",
        "a plain run must not write anything to stderr"
    );

    let stats = run_in(dir.path(), &["run", &src, "--stats"]);
    assert!(stats.status.success(), "stderr: {}", String::from_utf8_lossy(&stats.stderr));
    assert_eq!(String::from_utf8_lossy(&stats.stdout), "data\n");
    let stderr = String::from_utf8_lossy(&stats.stderr);
    assert!(
        stderr.contains("[steps=") && stderr.contains("stack="),
        "--stats should bring the run summary back, got: {stderr}"
    );

    let debug = Command::new(crush_run_bin())
        .args(["run", &src])
        .current_dir(dir.path())
        .stdin(Stdio::null())
        .env("CRUSH_DEBUG", "1")
        .output()
        .unwrap();
    assert!(debug.status.success(), "stderr: {}", String::from_utf8_lossy(&debug.stderr));
    assert!(
        String::from_utf8_lossy(&debug.stderr).contains("[steps="),
        "CRUSH_DEBUG=1 should bring the run summary back"
    );
}

/// `io.eprint(x)` writes to stderr, `io.print(x)` keeps writing to stdout, so
/// `script > data 2> diag` separates them.
#[test]
fn io_eprint_writes_to_stderr_and_leaves_stdout_alone() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_program(
        dir.path(),
        "split.crush",
        "fn main() {\n    io.print(\"out\")\n    io.eprint(\"diag: 1\")\n    io.print(\"out2\")\n}",
    );

    let out = run_in(dir.path(), &["run", &src]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "out\nout2\n",
        "stderr must not leak into stdout"
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr), "diag: 1\n");
}

/// A capability that is registered but failed is named, not "unknown": the
/// `unknown capability:` prefix is reserved for a name the VM has no handler
/// for at all (GitHub #370).
#[test]
fn failing_registered_capability_is_reported_by_its_own_name() {
    let dir = tempfile::tempdir().unwrap();
    let src = write_program(
        dir.path(),
        "missing.crush",
        "fn main() {\n    io.print(fs.cat(\"nope.txt\"))\n}",
    );

    let out = run_in(dir.path(), &["run", &src, "--fs", "--fs-root", "."]);
    assert!(!out.status.success(), "reading a missing file must fail");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("fs.cat") && stderr.contains("No such file"),
        "the failure should name the capability and the reason, got: {stderr}"
    );
    assert!(
        !stderr.contains("unknown capability"),
        "fs.cat is a registered capability; it must not read as unknown: {stderr}"
    );
}
