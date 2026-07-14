//! Polyglot execution is a CAPABILITY, not ambient authority.
//!
//! Crush is a capability-based language, but `@python`/`@bash`/`@javascript` blocks used to spawn
//! real interpreters with the host process's full authority and NO capability check — proven this
//! session: `@bash { touch /tmp/x }` wrote the file with zero grants. That contradicts the entire
//! premise of the language.
//!
//! `@lang` is now gated on a `polyglot.<lang>` host capability, exactly like fs.read/net.get. No
//! grant → the spawn is refused, loudly. `--polyglot` (crush-run) or the CapabilitySet→HostCaps
//! Enforcer (exo-light) grants it. These tests pin that gate.

use std::io::Write;
use std::process::Command;

fn crush_run_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}

fn run(src: &str, extra: &[&str]) -> (String, String, bool) {
    let dir = std::env::temp_dir().join(format!("crush_poly_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join(format!("t{}.crush", src.len()));
    write!(std::fs::File::create(&f).unwrap(), "{src}").unwrap();
    let mut args = vec!["run", f.to_str().unwrap()];
    args.extend_from_slice(extra);
    let out = Command::new(crush_run_bin()).args(&args).output().expect("crush-run");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

#[test]
fn bash_block_refused_without_grant() {
    // The escape. Must NOT spawn.
    let probe = std::env::temp_dir().join("crush_test_escape_probe");
    let _ = std::fs::remove_file(&probe);
    let src = format!("fn main() {{ @bash {{ touch {} }} }}", probe.display());
    let (out, err, ok) = run(&src, &[]);
    assert!(!ok, "program should FAIL without --polyglot");
    let combined = format!("{out}{err}");
    assert!(
        combined.contains("polyglot.bash") && combined.contains("requires"),
        "expected a loud polyglot-capability refusal, got: {combined}"
    );
    assert!(!probe.exists(), "SECURITY: @bash escaped the capability gate and wrote a file");
    let _ = std::fs::remove_file(&probe);
}

#[test]
fn python_block_refused_without_grant() {
    let (out, err, ok) = run("fn main() { @python { x = 1 } }", &[]);
    assert!(!ok);
    assert!(format!("{out}{err}").contains("polyglot.python"), "expected polyglot.python refusal");
}

#[test]
fn python_block_runs_with_grant() {
    // WITH --polyglot, the same block executes and marshals back.
    let (out, _err, ok) = run(
        "fn main() { let base = 5; @python { result = base * 2 } io.print(\"r=\" + result); }",
        &["--stdlib", "--polyglot"],
    );
    assert!(ok, "should succeed with --polyglot");
    assert!(out.contains("r=10"), "expected marshaled result, got: {out}");
}
