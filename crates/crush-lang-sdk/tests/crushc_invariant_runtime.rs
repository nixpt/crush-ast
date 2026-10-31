//! CRUSH-370: `crushc --invariant-runtime` emitted a call to a capability no
//! host provides; it is now a clear error rather than a program that dies later.

use std::process::Command;

#[test]
fn invariant_runtime_flag_is_a_clear_error() {
    let bin = option_env!("CARGO_BIN_EXE_crushc").unwrap_or("crushc");
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("p.crush");
    let out = d.path().join("p.cvm1");
    std::fs::write(&src, "fn main() { print(\"ran\") }\n").unwrap();
    let o = Command::new(bin)
        .args([src.to_str().unwrap(), "--invariant-runtime", "-o", out.to_str().unwrap()])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success());
    assert!(err.contains("--invariant-runtime was removed") && err.contains("invariant.evaluate"), "{err}");
    assert!(!out.exists(), "no artifact");
    // Without the flag the same program builds.
    let o = Command::new(bin).args([src.to_str().unwrap(), "-o", out.to_str().unwrap()]).output().unwrap();
    assert!(o.status.success() && out.exists());
}
