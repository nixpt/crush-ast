//! `crush-pkg new NAME --cli` (CRUSH-352): the scaffold builds and runs out
//! of the box. CI-safe: facet comes from `--facet-path` (a test double under
//! `tests/fixtures/mini-facet`), never from the network.

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

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini-facet")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn scaffold_runs_help_and_reports_bad_options_with_facets_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let facet = fixture();
    let out = crush_pkg(tmp.path(), &["new", "mytool", "--cli", "--facet-path", facet.to_str().unwrap()]);
    assert!(out.status.success(), "{}", text(&out));
    let app = tmp.path().join("mytool");

    let toml = std::fs::read_to_string(app.join("capsule.toml")).unwrap();
    assert!(toml.contains("category = \"cli\""), "{toml}");
    assert!(toml.contains("required = [\"io.print\", \"sys.args\", \"sys.exit\"]"), "{toml}");
    assert!(toml.contains("name = \"facet\"") && toml.contains("path = "), "{toml}");

    let help = crush_pkg(&app, &["run", "--", "--help"]);
    assert_eq!(help.status.code(), Some(0), "{}", text(&help));
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: mytool"), "{}", text(&help));
    assert!(!text(&help).contains("E-RUN"));

    let bogus = crush_pkg(&app, &["run", "--", "--bogus"]);
    assert_eq!(bogus.status.code(), Some(2), "{}", text(&bogus));
    assert!(text(&bogus).contains("unrecognized option '--bogus'"), "{}", text(&bogus));

    let plain = crush_pkg(&app, &["run", "--", "hi", "there"]);
    assert!(plain.status.success(), "{}", text(&plain));
    assert!(String::from_utf8_lossy(&plain.stdout).contains("hi there"), "{}", text(&plain));
}

#[test]
fn published_scaffold_pins_facet_as_a_git_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let out = crush_pkg(tmp.path(), &["new", "tool2", "--cli"]);
    assert!(out.status.success(), "{}", text(&out));
    let toml = std::fs::read_to_string(tmp.path().join("tool2/capsule.toml")).unwrap();
    assert!(toml.contains("git = \"https://github.com/nixpt/facet\""), "{toml}");
    assert!(toml.contains("tag = \"v0.1.1\""), "{toml}");
    // and the manifest it wrote parses back as a valid git dependency
    let m = crush_pkg::manifest::Manifest::from_file(&tmp.path().join("tool2/capsule.toml")).unwrap();
    assert_eq!(m.dependencies.len(), 1);
    assert_eq!(m.dependencies[0].tag.as_deref(), Some("v0.1.1"));
}

#[test]
fn facet_path_needs_cli_and_an_existing_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let no_cli = crush_pkg(tmp.path(), &["new", "a", "--facet-path", "/nonexistent"]);
    assert!(!no_cli.status.success());
    let missing = crush_pkg(tmp.path(), &["new", "b", "--cli", "--facet-path", "/nonexistent"]);
    assert!(!missing.status.success());
    assert!(text(&missing).contains("no capsule.toml there"), "{}", text(&missing));
}
