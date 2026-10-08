//! `crush-pkg check` compares the capabilities a package uses with the ones
//! `capsule.toml` declares (CRUSH-170), over the same program `build`
//! compiles, so packages with path deps check correctly.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn crush_pkg(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-pkg"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn crush-pkg")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The `E-CAPS` NDJSON records in `out`'s stdout, as (level, message).
fn caps_records(out: &Output) -> Vec<(String, String)> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["code"] == "E-CAPS")
        .map(|v| {
            (
                v["level"].as_str().unwrap().to_string(),
                v["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn package(root: &Path, name: &str, extra_toml: &str, source: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("capsule.toml"),
        format!(
            "[capsule]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n{extra_toml}"
        ),
    )
    .unwrap();
    std::fs::write(dir.join("src/main.crush"), source).unwrap();
    dir
}

/// `util` reads a file in `load()`, and has a `fetch()` nobody calls that
/// would need the network. `app` calls `load()`.
fn app_with_dep(root: &Path, app_caps: &str) -> PathBuf {
    package(
        root,
        "util",
        "",
        "fn load(path) -> string {\n    return fs.cat(path)\n}\n\n\
         fn fetch(url) -> string {\n    return net.http_get(url)\n}\n",
    );
    package(
        root,
        "app",
        &format!("{app_caps}\n[[dependencies]]\nname = \"util\"\npath = \"../util\"\n"),
        "fn main() {\n    io.print(load(\"notes.txt\"))\n}\n",
    )
}

#[test]
fn package_with_a_path_dep_checks_clean_when_declared() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(tmp.path(), "[capabilities]\nrequired = [\"fs.cat\"]\n");

    let out = crush_pkg(&app, &["check"]);
    let all = text(&out);
    // Before CRUSH-170, check compiled the entry alone and failed on `load`.
    assert!(out.status.success(), "check failed:\n{all}");
    assert!(
        all.contains("capabilities used: fs.cat, io.print"),
        "the dep's uncalled fetch() must not count:\n{all}"
    );
    assert!(!app.join("target").exists(), "check must not write target/");
}

#[test]
fn capability_used_through_a_dep_but_undeclared_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(tmp.path(), "");

    let out = crush_pkg(&app, &["check", "--message-format=json"]);
    assert!(!out.status.success(), "check passed:\n{}", text(&out));
    let records = caps_records(&out);
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, "error");
    assert!(records[0].1.contains("`fs.cat`"), "{records:?}");
    // The failure itself keeps the builder's code.
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"code\":\"E-BUILDER\""));
}

#[test]
fn declared_but_unused_capability_is_a_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(
        tmp.path(),
        "[capabilities]\nrequired = [\"fs\", \"env.get\", \"io.print\"]\n",
    );

    let out = crush_pkg(&app, &["check", "--message-format=json"]);
    assert!(
        out.status.success(),
        "warnings must not fail check:\n{}",
        text(&out)
    );
    let records = caps_records(&out);
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].0, "warning");
    assert!(records[0].1.contains("`env.get`"), "{records:?}");

    let human = crush_pkg(&app, &["check"]);
    assert!(
        text(&human)
            .contains("warning: capsule.toml:7: [capabilities] required declares `env.get`"),
        "{}",
        text(&human)
    );
}

#[test]
fn polyglot_block_needs_its_polyglot_capability() {
    let tmp = tempfile::tempdir().unwrap();
    let source = "fn main() {\n    @python {\nprint(1)\n}\n}\n";
    let bare = package(tmp.path(), "py", "", source);
    let out = crush_pkg(&bare, &["check", "--message-format=json"]);
    assert!(!out.status.success());
    let records = caps_records(&out);
    assert!(
        records
            .iter()
            .any(|(l, m)| l == "error" && m.contains("`polyglot.python`")),
        "{records:?}"
    );

    let declared = package(
        tmp.path(),
        "py2",
        "[capabilities]\nrequired = [\"polyglot.python\"]\n",
        source,
    );
    let out = crush_pkg(&declared, &["check"]);
    assert!(out.status.success(), "{}", text(&out));
}

#[test]
fn web_capsule_is_warned_about_capabilities_the_browser_lacks() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = package(
        tmp.path(),
        "webby",
        "platforms = [\"linux\", \"web\"]\n\n[capabilities]\nrequired = [\"env.get\"]\n",
        "fn main() {\n    io.print(str.len(\"x\"))\n    io.print(env.get(\"HOME\"))\n}\n",
    );
    let out = crush_pkg(&dir, &["check", "--message-format=json"]);
    assert!(out.status.success(), "{}", text(&out));
    let records = caps_records(&out);
    assert_eq!(
        records.len(),
        1,
        "only env.get is missing in the browser: {records:?}"
    );
    assert_eq!(records[0].0, "warning");
    assert!(records[0].1.contains("\"web\"") && records[0].1.contains("`env.get`"));
}
