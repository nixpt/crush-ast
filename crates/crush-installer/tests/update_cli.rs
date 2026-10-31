//! The `crush-installer update` process contract (CRUSH-344): `--check` exit
//! codes and the cargo-install refusal, observed on the real binary. The
//! release source is a `file://` fixture (no network).
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const TARGET: &str = "x86_64-unknown-linux-gnu";

fn fake_crush(dir: &Path, ver: &str) {
    let p = dir.join("crush");
    fs::write(&p, format!("#!/bin/sh\necho \"crush {ver}\"\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

fn releases(root: &Path, versions: &[&str]) {
    let list: Vec<_> = versions
        .iter()
        .map(|v| {
            serde_json::json!({
                "tag_name": format!("v{v}"), "draft": false, "prerelease": false,
                "assets": [{"name": format!("crush-{v}-{TARGET}.tar.gz")}, {"name": "SHA256SUMS"}],
            })
        })
        .collect();
    fs::write(
        root.join("releases.json"),
        serde_json::to_vec(&list).unwrap(),
    )
    .unwrap();
}

fn update(root: &Path, cargo_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-installer"))
        .arg("update")
        .args(args)
        .env(
            "CRUSH_UPDATE_RELEASES_URL",
            format!("file://{}", root.join("releases.json").display()),
        )
        .env(
            "CRUSH_UPDATE_DOWNLOAD_BASE",
            format!("file://{}", root.display()),
        )
        .env("CARGO_HOME", cargo_home)
        .env_remove("GITHUB_TOKEN")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn check_exits_10_when_an_update_exists_and_0_when_up_to_date() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    releases(root.path(), &["0.8.0", "0.9.0"]);
    let d = dir.path().to_str().unwrap();

    fake_crush(dir.path(), "0.8.0");
    let out = update(root.path(), home.path(), &["--check", "--dir", d]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(10),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("update available: 0.8.0 -> 0.9.0"),
        "{stdout}"
    );

    fake_crush(dir.path(), "0.9.0");
    let out = update(root.path(), home.path(), &["--check", "--dir", d]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );

    // A pinned older version is "available" to --check, and --check still wrote nothing.
    let out = update(
        root.path(),
        home.path(),
        &["--check", "--dir", d, "--version", "0.8.0"],
    );
    assert_eq!(out.status.code(), Some(10));
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec!["crush"]);
}

#[test]
fn cargo_managed_dir_is_refused_with_the_cargo_hint() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    releases(root.path(), &["0.9.0"]);
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fake_crush(&bin, "0.8.0");
    let out = update(root.path(), home.path(), &["--dir", bin.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("installed with cargo; run: cargo install crush-lang-sdk"),
        "{stderr}"
    );
}
