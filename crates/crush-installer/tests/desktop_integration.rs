//! `install --desktop-integration` / `uninstall` on the real binary, with a
//! throwaway `$XDG_DATA_HOME`/`$HOME` and a fake `update-mime-database` on
//! `PATH` that records its argument (CRUSH-366).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

const BINS: &[&str] = &[
    "crushc",
    "crush-run",
    "crush-compile",
    "crush-repl",
    "crush-pkg",
    "crush",
    "crush-debug",
];

fn installer(root: &Path, args: &[&str], with_mime_tool: bool) -> Output {
    let tools = root.join("tools");
    let path = if with_mime_tool {
        format!("{}:/usr/bin:/bin", tools.display())
    } else {
        // no update-mime-database anywhere: an empty dir
        fs::create_dir_all(root.join("empty")).unwrap();
        root.join("empty").display().to_string()
    };
    Command::new(env!("CARGO_BIN_EXE_crush-installer"))
        .args(args)
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("xdg"))
        .env("PATH", path)
        .output()
        .unwrap()
}

fn setup(root: &Path) {
    let bins = root.join("bins");
    fs::create_dir_all(&bins).unwrap();
    for b in BINS {
        fs::write(bins.join(b), "#!/bin/sh\n").unwrap();
    }
    fs::create_dir_all(root.join("home")).unwrap();
    let tools = root.join("tools");
    fs::create_dir_all(&tools).unwrap();
    let fake = tools.join("update-mime-database");
    fs::write(
        &fake,
        format!("#!/bin/sh\necho \"$1\" >> {}/mime-db.log\n", root.display()),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
}

fn base<'a>(root: &'a Path, prefix: &'a Path) -> Vec<String> {
    vec![
        "install".into(),
        "--prefix".into(),
        prefix.display().to_string(),
        "--bin-dir".into(),
        root.join("bins").display().to_string(),
        "--no-path".into(),
        "--desktop-integration".into(),
    ]
}

#[test]
fn install_then_uninstall() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    setup(root);
    let prefix = root.join("prefix");
    let args = base(root, &prefix);
    let out = installer(
        root,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        true,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let xml = root.join("xdg/mime/packages/crush.xml");
    let magic = root.join("xdg/crush/crush.magic");
    assert!(
        fs::read_to_string(&xml)
            .unwrap()
            .contains("application/x-crush-cvm1")
    );
    assert!(fs::read_to_string(&magic).unwrap().contains("CVM1"));
    let log = fs::read_to_string(root.join("mime-db.log")).unwrap();
    assert_eq!(log.trim(), root.join("xdg/mime").display().to_string());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("~/.magic"), "printed hint missing: {err}");
    // the home directory itself is untouched
    assert!(!root.join("home/.magic").exists());

    let out = installer(
        root,
        &["uninstall", "--prefix", prefix.to_str().unwrap()],
        true,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!xml.exists() && !magic.exists());
    assert!(!root.join("xdg/crush").exists());
    // the db is refreshed again on uninstall
    assert_eq!(
        fs::read_to_string(root.join("mime-db.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[test]
fn works_without_update_mime_database() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    setup(root);
    let prefix = root.join("prefix");
    let args = base(root, &prefix);
    let out = installer(
        root,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        false,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(root.join("xdg/mime/packages/crush.xml").exists());
    assert!(String::from_utf8_lossy(&out.stderr).contains("update-mime-database not found"));
}

#[test]
fn off_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    setup(root);
    let prefix = root.join("prefix");
    let mut args = base(root, &prefix);
    args.pop();
    let out = installer(
        root,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        true,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!root.join("xdg").exists());
}
