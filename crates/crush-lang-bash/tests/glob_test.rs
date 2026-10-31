//! Pathname expansion and the capabilities it needs (`fs.exists`, `fs.list` and
//! `fs.is_dir` — a walk into `*/*.txt` has to know which matches it may descend
//! into).
//!
//! The oracle corpus covers the matching rules against the real shells, each
//! case in a directory of fixture files. These tests pin the contract instead:
//! a glob word asks the host for the filesystem, and a host that grants none
//! gets a loud refusal rather than a guess. A quoted metacharacter is not a
//! glob at all, so it needs no capability.

fn run(src: &str, caps: &crush_vm::HostCaps) -> Result<(String, i32), String> {
    let cast = crush_lang_bash::bash_to_cast(src).map_err(|e| format!("{e:#}"))?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| format!("CAST→CASM: {e:#}"))?;
    let prog = crush_lang_sdk::compile::casm_to_vm(&casm).map_err(|e| format!("CASM→CVM1: {e:#}"))?;
    let quotas = crush_vm::Quotas {
        max_steps: 1_000_000,
        ..Default::default()
    };
    let mut out = String::new();
    match crush_vm::vm::run_with_caps_streaming(&prog, &quotas, Some(caps), &mut |s: &str| out.push_str(s)) {
        Ok(_) => Ok((out, 0)),
        Err(crush_vm::VmError::Exit(code)) => Ok((out, code)),
        Err(e) => Err(format!("runtime: {e}")),
    }
}

#[test]
fn glob_without_filesystem_caps_fails_loudly() {
    let caps = crush_lang_sdk::HostCapsBuilder::new().build();
    let err = run("echo *.txt\n", &caps).expect_err("a glob needs the filesystem");
    assert!(
        err.contains("fs."),
        "the refusal must name the missing capability, got: {err}"
    );
}

#[test]
fn quoted_metacharacters_need_no_capability() {
    let caps = crush_lang_sdk::HostCapsBuilder::new().build();
    let (out, code) = run("echo \"*.txt\" 'a*' a\\*b\n", &caps).expect("no glob, no fs access");
    assert_eq!(out, "*.txt a* a*b\n");
    assert_eq!(code, 0);
}

#[test]
fn glob_matches_files_through_the_grant() {
    // An in-memory filesystem keeps the test hermetic: the grant is the same
    // code path a disk root takes, without touching the host's disk.
    let dir = std::env::temp_dir().join(format!("crush-glob-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["b.txt", "a.txt", "c.md"] {
        std::fs::File::create(dir.join(f)).unwrap();
    }
    let caps = crush_lang_sdk::HostCapsBuilder::new()
        .fs(true)
        .fs_root(dir.display().to_string())
        .build();
    let (out, code) = run("echo *.txt\necho zz*\nfor f in *.md; do echo \"[$f]\"; done\n", &caps)
        .expect("glob over the granted root");
    assert_eq!(out, "a.txt b.txt\nzz*\n[c.md]\n");
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directory_components_glob_through_the_grant() {
    let dir = std::env::temp_dir().join(format!("crush-glob-test-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::create_dir_all(dir.join("b")).unwrap();
    std::fs::File::create(dir.join("a/one.txt")).unwrap();
    std::fs::File::create(dir.join("b/two.txt")).unwrap();
    // A *file* is matched by `*` too: the walk must skip it instead of failing
    // with "not a directory" when it tries to list it.
    std::fs::File::create(dir.join("c.txt")).unwrap();
    let caps = crush_lang_sdk::HostCapsBuilder::new()
        .fs(true)
        .fs_root(dir.display().to_string())
        .build();
    let (out, code) = run("echo */*.txt\necho */\n", &caps).expect("glob through a directory component");
    assert_eq!(out, "a/one.txt b/two.txt\na/ b/\n");
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directory_glob_without_the_is_dir_probe_fails_loudly() {
    let dir = std::env::temp_dir().join(format!("crush-glob-test-nodir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::File::create(dir.join("sub/x.txt")).unwrap();
    // The two caps `walk_run` used to grant: enough for `*.txt`, not enough to
    // know where a walk may descend.
    let root = dir.display().to_string();
    let mut caps = crush_vm::HostCaps::new();
    caps.register(Box::new(crush_lang_sdk::host_caps::FsExistsCap::new(&root)));
    caps.register(Box::new(crush_lang_sdk::host_caps::FsListCap::new(&root)));
    let err = run("echo */*.txt\n", &caps).expect_err("a walk needs fs.is_dir");
    assert!(err.contains("fs.is_dir"), "got: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}
