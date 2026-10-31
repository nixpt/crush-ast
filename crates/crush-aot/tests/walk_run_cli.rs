//! `crush-walk-run` CLI behaviour: output printed before a runtime error is
//! kept, and a runtime error exits non-zero (CRUSH-184).

use std::process::Command;

/// Run `crush-walk-run` on `source` (written to a temp file with extension
/// `ext`) and return the process output.
fn run_walk_run(name: &str, ext: &str, source: &str) -> std::process::Output {
    let path = std::env::temp_dir().join(format!(
        "crush_walk_run_{}_{name}.{ext}",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_crush-walk-run"))
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    out
}

#[test]
fn prior_output_is_kept_and_a_runtime_error_exits_nonzero() {
    // Prints 1, 2, 3, then reads past the end.
    let out = run_walk_run(
        "loop_error",
        "py",
        "xs = [1, 2, 3]\ni = 0\nwhile i < 10:\n    print(xs[i])\n    i = i + 1\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1\n2\n3\n"),
        "output printed before the error must survive, got stdout: {stdout:?}"
    );
    assert!(
        !out.status.success(),
        "a runtime error must exit non-zero, got {:?}",
        out.status.code()
    );
}

#[test]
fn a_successful_run_exits_zero() {
    let out = run_walk_run("ok", "py", "print(\"ok\")\n");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "ok\n");
    assert!(out.status.success());
}
