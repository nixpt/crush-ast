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
    // crush-run refuses before running (CRUSH-243); the VM's own EXEC_LANG gate is
    // pinned by the embedded-runtime tests below, which have no pre-run check.
    assert!(
        combined.contains("polyglot.bash")
            && (combined.contains("requires") || combined.contains("nothing was run")),
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

// ── CRUSH-104: the gate must ship in the published SDK ──────────────────────────────────────────
//
// crates.io's crush-lang-sdk 0.2.0 depends on crush-vm 0.2.0, whose EXEC_LANG did
// `Command::new(lang).arg("-c").arg(code)` with no gate and no allowlist: ANY `@word { … }` ran a
// PATH binary named `word`. `@zsh { touch probe }` became `zsh -c "touch probe"`. These tests pin
// both halves of the fix, on the CLI path (crush-run) and on the embedding path (Runtime +
// HostCapsBuilder) that SDK users call directly:
//   1. no grant → every @lang block is refused, including names outside the allowlist;
//   2. a language outside the allowlist is refused even with every polyglot grant.

fn escape_probe(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("crush_104_probe_{tag}_{}", std::process::id()))
}

fn assert_refused_without_spawn(probe: &std::path::Path, ok: bool, combined: &str) {
    assert!(!ok, "program should be refused, got success: {combined}");
    // Refused by a capability gate, not by something unrelated (a parse error would also
    // fail the program and leave no probe, proving nothing about the gate): the VM's
    // EXEC_LANG gate, or crush-run's pre-run check naming the `polyglot.<lang>` grant
    // (CRUSH-243).
    assert!(
        combined.contains("requires the 'polyglot.")
            || combined.contains("no executor registered for language")
            || (combined.contains("nothing was run") && combined.contains("polyglot.")),
        "expected an EXEC_LANG gate/allowlist refusal, got: {combined}"
    );
    assert!(
        !probe.exists(),
        "SECURITY: an @lang block spawned a host process and wrote {}",
        probe.display()
    );
}

#[test]
fn unknown_lang_block_refused_without_grant() {
    let probe = escape_probe("cli_nogrant");
    let _ = std::fs::remove_file(&probe);
    let src = format!("fn main() {{ @zsh {{ touch {} }} }}", probe.display());
    let (out, err, ok) = run(&src, &[]);
    assert_refused_without_spawn(&probe, ok, &format!("{out}{err}"));
}

#[test]
fn unknown_lang_block_refused_even_with_polyglot() {
    let probe = escape_probe("cli_grant");
    let _ = std::fs::remove_file(&probe);
    let src = format!("fn main() {{ @zsh {{ touch {} }} }}", probe.display());
    let (out, err, ok) = run(&src, &["--polyglot"]);
    let combined = format!("{out}{err}");
    assert_refused_without_spawn(&probe, ok, &combined);
    assert!(
        combined.contains("zsh"),
        "expected the refusal to name the language, got: {combined}"
    );
}

fn run_embedded(src: &str, caps: crush_lang_sdk::HostCaps) -> Result<String, String> {
    let program = crush_lang_sdk::compile::compile_crush_source(src).map_err(|e| e.to_string())?;
    crush_lang_sdk::Runtime::new()
        .with_host_caps(caps)
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

#[test]
fn embedded_runtime_refuses_lang_blocks_without_grant() {
    use crush_lang_sdk::HostCapsBuilder;
    for lang in ["bash", "python", "zsh"] {
        let probe = escape_probe(&format!("lib_nogrant_{lang}"));
        let _ = std::fs::remove_file(&probe);
        let body = if lang == "python" {
            format!("open('{}', 'w').close()", probe.display())
        } else {
            format!("touch {}", probe.display())
        };
        let src = format!("fn main() {{ @{lang} {{ {body} }} }}");
        let res = run_embedded(&src, HostCapsBuilder::new().build());
        let msg = format!("{res:?}");
        assert_refused_without_spawn(&probe, res.is_ok(), &msg);
    }
}

#[test]
fn embedded_runtime_refuses_unknown_lang_with_every_polyglot_grant() {
    use crush_lang_sdk::HostCapsBuilder;
    let probe = escape_probe("lib_grant_zsh");
    let _ = std::fs::remove_file(&probe);
    let src = format!("fn main() {{ @zsh {{ touch {} }} }}", probe.display());
    let caps = HostCapsBuilder::new()
        .polyglot(&["python", "javascript", "bash"])
        .build();
    let res = run_embedded(&src, caps);
    let msg = format!("{res:?}");
    assert_refused_without_spawn(&probe, res.is_ok(), &msg);
}
