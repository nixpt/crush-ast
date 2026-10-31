//! Import mode: a package whose entry uses `import` compiles file by file,
//! with `src/` and every dependency (by name) as import roots — no source
//! concatenation. Git dependencies are local `file://` repos (no network).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crush_pkg::Manifest;
use crush_pkg::builder::PackageBuilder;

fn write(root: &Path, files: &[(&str, &str)]) {
    for (rel, src) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn crush_pkg(dir: &Path, cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-pkg"))
        .args(args)
        .current_dir(dir)
        .env("CRUSH_PKG_CACHE", cache)
        .env_remove("CRUSH_PKG_OFFLINE")
        .env_remove("CRUSH_PATH")
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-pkg")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn builder(root: &Path) -> PackageBuilder {
    let m = Manifest::from_file(&root.join("capsule.toml")).unwrap();
    PackageBuilder::new(m, root.to_path_buf())
}

const APP_TOML: &str = "[capsule]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n";

/// A tagged git repo whose entry file is NOT named after the package
/// (`gd.crush` for package `gdlib`), so only the by-name mapping finds it.
fn lib_repo(root: &Path) -> PathBuf {
    let repo = root.join("gdlib");
    write(
        &repo,
        &[
            ("capsule.toml", "[capsule]\nname = \"gdlib\"\nversion = \"0.1.0\"\nentry = \"gd.crush\"\nlanguage = \"crush\"\ncategory = \"library\"\n"),
            ("gd.crush", "fn gd_greet() {\n    return \"hello from gdlib\"\n}\n"),
        ],
    );
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "v1"]);
    git(&repo, &["tag", "-a", "v1", "-m", "v1"]);
    repo
}

#[test]
fn git_dependency_is_imported_by_name_without_concatenation() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = lib_repo(tmp.path());
    let app = tmp.path().join("app");
    write(
        &app,
        &[
            (
                "capsule.toml",
                &format!("{APP_TOML}\n[[dependencies]]\nname = \"gdlib\"\ngit = \"file://{}\"\ntag = \"v1\"\n", repo.display()),
            ),
            ("src/main.crush", "import gdlib { gd_greet }\n\nfn main() {\n    io.print(gd_greet())\n}\n"),
        ],
    );
    let cache = tmp.path().join("cache");
    let out = crush_pkg(&app, &cache, &["run"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello from gdlib"), "{}", text(&out));
    let lock = std::fs::read_to_string(app.join("capsule.lock")).unwrap();
    assert!(lock.contains("name = \"gdlib\"") && lock.contains("ref = \"tag:v1\""), "{lock}");

    // The function arrived through the import (module-qualified), not pasted in.
    // SAFETY of env: this test binary sets CRUSH_PKG_CACHE only here.
    unsafe { std::env::set_var("CRUSH_PKG_CACHE", &cache) };
    let output = builder(&app).build().unwrap();
    assert!(output.functions.iter().any(|f| f == "gdlib::gd_greet"), "{:?}", output.functions);
    assert!(!output.functions.iter().any(|f| f == "gd_greet"), "{:?}", output.functions);
}

#[test]
fn multi_module_src_tree_resolves_from_src_root() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("app");
    write(
        &app,
        &[
            ("capsule.toml", APP_TOML),
            ("src/main.crush", "import cli\n\nfn main() {\n    io.print(cli.banner(\"crush\"))\n}\n"),
            // src/cli.crush reaches src/util/strings.crush; src/util/strings.crush
            // reaches back up to src/ (only possible with src/ as a search root).
            ("src/cli.crush", "import util.strings as s\n\nfn banner(x) {\n    return s.shout(x)\n}\n"),
            ("src/util/strings.crush", "import marks\n\nfn shout(x) {\n    return x + marks.bang()\n}\n"),
            ("src/marks.crush", "fn bang() {\n    return \"!\"\n}\n"),
        ],
    );
    let out = crush_pkg(&app, &tmp.path().join("cache"), &["run"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("crush!"), "{}", text(&out));
}

#[test]
fn path_dependency_with_dashed_name_and_src_lib_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("my-lib");
    crush_pkg::manifest::scaffold_lib_package(&lib, "my-lib").unwrap();
    write(&lib, &[("src/extra/twice.crush", "fn twice(x) {\n    return x + x\n}\n")]);
    let app = tmp.path().join("app");
    write(
        &app,
        &[
            ("capsule.toml", &format!("{APP_TOML}\n[[dependencies]]\nname = \"my-lib\"\npath = \"../my-lib\"\n")),
            (
                "src/main.crush",
                "import my_lib { greet }\nimport my_lib.extra.twice as t\n\nfn main() {\n    io.print(t.twice(greet(\"x\")))\n}\n",
            ),
        ],
    );
    let out = crush_pkg(&app, &tmp.path().join("cache"), &["run"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello, xhello, x"), "{}", text(&out));
}

#[test]
fn build_file_compiles_a_test_that_imports_the_package_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("my-app");
    crush_pkg::manifest::scaffold_package(&app, "my-app").unwrap();
    let output = builder(&app).build_file(&app.join("tests/greeting.crush")).unwrap();
    assert!(output.functions.iter().any(|f| f == "my_app::greeting"), "{:?}", output.functions);
}

#[test]
fn concatenation_mode_still_serves_packages_without_imports() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("lib");
    write(
        &lib,
        &[
            ("capsule.toml", "[capsule]\nname = \"lib\"\nversion = \"0.1.0\"\nentry = \"lib.crush\"\nlanguage = \"crush\"\n"),
            ("lib.crush", "fn helper() {\n    return \"bare call ok\"\n}\n"),
        ],
    );
    let app = tmp.path().join("app");
    write(
        &app,
        &[
            ("capsule.toml", &format!("{APP_TOML}\n[[dependencies]]\nname = \"lib\"\npath = \"../lib\"\n")),
            ("src/main.crush", "fn main() {\n    io.print(helper())\n}\n"),
        ],
    );
    assert!(!crush_pkg::builder::uses_imports(&app.join("src/main.crush")));
    let out = crush_pkg(&app, &tmp.path().join("cache"), &["run"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("bare call ok"), "{}", text(&out));
}

#[test]
fn uses_imports_ignores_comments_and_external_forms() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("m.crush");
    std::fs::write(&f, "// import facet\nimport @http \"x\"\nfn main() {\n}\n").unwrap();
    assert!(!crush_pkg::builder::uses_imports(&f));
    std::fs::write(&f, "import facet { facet_run }\nfn main() {\n}\n").unwrap();
    assert!(crush_pkg::builder::uses_imports(&f));
}

#[test]
fn new_scaffolds_the_standard_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cache");
    for (name, extra) in [("app", None), ("lib1", Some("--lib"))] {
        let mut args = vec!["new", name];
        args.extend(extra);
        let out = crush_pkg(tmp.path(), &cache, &args);
        assert!(out.status.success(), "{}", text(&out));
        let dir = tmp.path().join(name);
        assert_eq!(std::fs::read_to_string(dir.join(".gitignore")).unwrap(), "target/\n");
        let tests: Vec<_> = std::fs::read_dir(dir.join("tests")).unwrap().collect();
        assert_eq!(tests.len(), 1, "{name}: one sample test");
        let m = Manifest::from_file(&dir.join("capsule.toml")).unwrap();
        let entry = if extra.is_some() { "src/lib.crush" } else { "src/main.crush" };
        assert_eq!(m.capsule.entry, entry);
        assert!(dir.join(entry).is_file());
        let build = crush_pkg(&dir, &cache, &["build"]);
        assert!(build.status.success(), "{}", text(&build));
    }
    let both = crush_pkg(tmp.path(), &cache, &["new", "x", "--cli", "--lib"]);
    assert_eq!(both.status.code(), Some(2), "{}", text(&both));
}
