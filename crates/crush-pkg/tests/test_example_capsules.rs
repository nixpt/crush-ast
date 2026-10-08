//! The example capsules under `examples/crush/capsules/` build and run
//! through the real `crush-pkg` binary, under exactly the grants they need
//! (CRUSH-172). Each test copies the capsule into a temp dir first, so
//! `build` writes `target/` there and never into the source tree.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crush/capsules")
        .join(name)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() != "target" {
                copy_tree(&entry.path(), &dest);
            }
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}

fn crush_pkg(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_crush-pkg"));
    cmd.args(args)
        .current_dir(dir)
        .env_remove("BRIDGE_PEEK_FILE");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn crush-pkg")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn bridge_peek() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("squad-bridge-peek");
    copy_tree(&example("squad-bridge-peek"), &dir);
    (tmp, dir)
}

const LAST_FIVE: &[&str] = &[
    "[09:05] beta: claimed CRUSH-902",
    "[09:41] alpha: ✅ CRUSH-901 ready — PR #1",
    "[09:58] beta: blocked on CRUSH-901, waiting for merge",
    "[10:03] foreman: merged #1",
    "[10:07] beta: ✅ CRUSH-902 ready — PR #2",
];

fn assert_peek(all: &str) {
    assert!(
        all.contains("=== squad-bridge-peek: bridge.md (last 5 of 7) ==="),
        "{all}"
    );
    for line in LAST_FIVE {
        assert!(all.contains(line), "missing {line:?}:\n{all}");
    }
    // The two oldest entries are outside the peek.
    assert!(!all.contains("wave 3 open"), "{all}");
    assert!(!all.contains("claimed CRUSH-901"), "{all}");
}

#[test]
fn squad_bridge_peek_builds_and_runs_under_a_scoped_fs_grant() {
    let (_tmp, dir) = bridge_peek();

    let build = crush_pkg(&dir, &["build"], &[]);
    assert!(build.status.success(), "build failed:\n{}", text(&build));
    assert!(dir.join("target/squad-bridge-peek.cvm").is_file());

    let run = crush_pkg(
        &dir,
        &["run", "--fs", "--fs-root", "fixtures", "--env"],
        &[],
    );
    assert!(run.status.success(), "run failed:\n{}", text(&run));
    assert_peek(&text(&run));

    // Bare `crush-pkg` (build, then run the built program) takes the same
    // grants, and the file can come from the environment.
    let bare = crush_pkg(
        &dir,
        &["--fs", "--fs-root", "fixtures", "--env"],
        &[("BRIDGE_PEEK_FILE", "bridge.md")],
    );
    assert!(bare.status.success(), "bare failed:\n{}", text(&bare));
    assert_peek(&text(&bare));
}

#[test]
fn squad_bridge_peek_is_refused_without_its_grants() {
    let (_tmp, dir) = bridge_peek();

    let no_env = crush_pkg(&dir, &["run", "--fs", "--fs-root", "fixtures"], &[]);
    assert!(
        !no_env.status.success(),
        "ran without --env:\n{}",
        text(&no_env)
    );
    assert!(text(&no_env).contains("env.get"), "{}", text(&no_env));

    let no_fs = crush_pkg(&dir, &["run", "--env"], &[]);
    assert!(
        !no_fs.status.success(),
        "ran without --fs:\n{}",
        text(&no_fs)
    );
    assert!(text(&no_fs).contains("fs.cat"), "{}", text(&no_fs));
}

#[test]
fn squad_bridge_peek_cannot_read_outside_fs_root() {
    let (_tmp, dir) = bridge_peek();
    // A file that exists, one level above the sandbox root.
    for path in ["../capsule.toml", "/etc/hostname"] {
        let out = crush_pkg(
            &dir,
            &["run", "--fs", "--fs-root", "fixtures", "--env"],
            &[("BRIDGE_PEEK_FILE", path)],
        );
        let all = text(&out);
        assert!(
            !out.status.success(),
            "read {path} outside --fs-root:\n{all}"
        );
        assert!(
            all.contains("escapes sandbox root") || all.contains("absolute paths are not allowed"),
            "{all}"
        );
        assert!(!all.contains("[capsule]"), "capsule.toml leaked:\n{all}");
    }
}

#[test]
fn squad_bridge_peek_declares_exactly_the_capabilities_it_uses() {
    let (_tmp, dir) = bridge_peek();
    let out = crush_pkg(&dir, &["check", "--message-format=json"], &[]);
    let all = text(&out);
    assert!(out.status.success(), "check failed:\n{all}");
    assert!(!all.contains("E-CAPS"), "capability findings:\n{all}");

    let human = crush_pkg(&dir, &["check"], &[]);
    assert!(
        text(&human).contains("capabilities used: env.get, fs.cat, io.print,"),
        "{}",
        text(&human)
    );
}
