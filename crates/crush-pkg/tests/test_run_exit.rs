//! `crush-pkg run` runs the same combined program `build` produces
//! (CRUSH-348), and `sys.exit(n)` ends the process with status `n`, keeping
//! the output printed before it (CRUSH-349).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn crush_pkg(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-pkg"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-pkg")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn all(out: &Output) -> String {
    format!("{}{}", stdout(out), String::from_utf8_lossy(&out.stderr))
}

/// The CRUSH-348 repro: `consumer` calls `dep_hello()`, defined only in the
/// path dependency `dep`.
fn consumer_with_dep(root: &Path) -> PathBuf {
    let dep = root.join("dep");
    std::fs::create_dir_all(&dep).unwrap();
    std::fs::write(
        dep.join("capsule.toml"),
        "[capsule]\nname = \"dep\"\nversion = \"0.1.0\"\nentry = \"dep.crush\"\nlanguage = \"crush\"\ncategory = \"library\"\n",
    )
    .unwrap();
    std::fs::write(dep.join("dep.crush"), "fn dep_hello() {\n    return \"hello from dep\"\n}\n").unwrap();

    let consumer = root.join("consumer");
    std::fs::create_dir_all(&consumer).unwrap();
    std::fs::write(
        consumer.join("capsule.toml"),
        "[capsule]\nname = \"consumer\"\nversion = \"0.1.0\"\nentry = \"main.crush\"\nlanguage = \"crush\"\ncategory = \"cli\"\n\n[[dependencies]]\nname = \"dep\"\npath = \"../dep\"\n",
    )
    .unwrap();
    std::fs::write(consumer.join("main.crush"), "fn main() {\n    print(dep_hello())\n}\n").unwrap();
    consumer
}

/// A one-file CLI capsule whose `main` is `body`.
fn probe(root: &Path, body: &str) -> PathBuf {
    let dir = root.join("probe");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("capsule.toml"),
        "[capsule]\nname = \"probe\"\nversion = \"0.1.0\"\nentry = \"main.crush\"\nlanguage = \"crush\"\ncategory = \"cli\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.crush"), format!("fn main() {{\n{body}\n}}\n")).unwrap();
    dir
}

#[test]
fn run_sees_path_dependencies_like_build_and_bare() {
    let tmp = tempfile::tempdir().unwrap();
    let app = consumer_with_dep(tmp.path());

    for args in [&["run"][..], &[][..]] {
        let out = crush_pkg(&app, args);
        assert!(out.status.success(), "{args:?} failed:\n{}", all(&out));
        assert!(stdout(&out).contains("hello from dep"), "{args:?}:\n{}", all(&out));
    }
    // `run` now also writes target/ (it is the build-then-run path).
    assert!(app.join("target/consumer.cvm").is_file());
}

#[test]
fn run_reports_compile_errors_with_the_builder_code() {
    let tmp = tempfile::tempdir().unwrap();
    let app = consumer_with_dep(tmp.path());
    std::fs::write(app.join("main.crush"), "fn main() {\n    print(nope())\n}\n").unwrap();
    let out = crush_pkg(&app, &["run"]);
    assert!(!out.status.success());
    assert!(all(&out).contains("crush-pkg[E-BUILDER]"), "{}", all(&out));
}

#[test]
fn sys_exit_zero_is_success_and_keeps_output() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = probe(tmp.path(), "    print(\"before exit\")\n    sys.exit(0)");
    for args in [&["run"][..], &[][..]] {
        let out = crush_pkg(&dir, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}:\n{}", all(&out));
        assert!(stdout(&out).contains("before exit"), "{args:?}:\n{}", all(&out));
        assert!(!all(&out).contains("E-RUN"), "{args:?}:\n{}", all(&out));
    }
}

#[test]
fn sys_exit_n_is_the_process_status_without_a_crash_banner() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = probe(tmp.path(), "    print(\"usage: probe\")\n    sys.exit(2)");
    for args in [&["run"][..], &[][..]] {
        let out = crush_pkg(&dir, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}:\n{}", all(&out));
        assert!(stdout(&out).contains("usage: probe"), "{args:?}:\n{}", all(&out));
        assert!(!all(&out).contains("E-RUN"), "{args:?}:\n{}", all(&out));
    }
}

#[test]
fn genuine_runtime_errors_keep_e_run_and_earlier_output() {
    let tmp = tempfile::tempdir().unwrap();
    // Compiles, fails at run time: indexing an empty list.
    let dir = probe(tmp.path(), "    print(\"partial\")\n    let xs = []\n    print(xs[3])");
    for args in [&["run"][..], &[][..]] {
        let out = crush_pkg(&dir, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}:\n{}", all(&out));
        assert!(all(&out).contains("crush-pkg[E-RUN]"), "{args:?}:\n{}", all(&out));
        assert!(stdout(&out).contains("partial"), "{args:?}:\n{}", all(&out));
    }
}
