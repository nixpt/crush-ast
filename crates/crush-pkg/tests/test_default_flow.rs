//! Bare `crush-pkg` (build → write target/ → run) and the Crush-only guard
//! on `build`/`check` (CRUSH-167, folded in from squeeze).

use std::path::Path;
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

/// `app` calls `greet()`, which only exists in the path dep `util`.
fn app_with_dep(root: &Path) -> std::path::PathBuf {
    let util = root.join("util");
    std::fs::create_dir_all(util.join("src")).unwrap();
    std::fs::write(
        util.join("capsule.toml"),
        "[capsule]\nname = \"util\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n",
    )
    .unwrap();
    std::fs::write(
        util.join("src/main.crush"),
        "fn greet() -> string {\n    return \"hi from util\"\n}\n",
    )
    .unwrap();

    let app = root.join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(
        app.join("capsule.toml"),
        "[capsule]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n\n\
         [[dependencies]]\nname = \"util\"\npath = \"../util\"\n",
    )
    .unwrap();
    std::fs::write(app.join("src/main.crush"), "fn main() {\n    io.print(greet())\n}\n").unwrap();
    app
}

#[test]
fn bare_builds_writes_target_and_runs_the_built_program() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(tmp.path());

    let out = crush_pkg(&app, &[]);
    let all = text(&out);
    assert!(out.status.success(), "bare crush-pkg failed:\n{all}");
    assert!(all.contains("building app v0.1.0"), "{all}");
    assert!(all.contains("hi from util"), "program output missing:\n{all}");
    assert!(app.join("target/app.cvm").is_file(), "target/app.cvm not written");
    assert!(app.join("target/app.casm.json").is_file());

    // Control: `run` compiles the entry alone, so the dep's function is
    // missing. The bare flow above must have run the built program.
    let run = crush_pkg(&app, &["run"]);
    assert!(!run.status.success(), "control: `run` unexpectedly passed");
    assert!(text(&run).contains("greet"), "{}", text(&run));
}

#[test]
fn bare_reports_compile_errors_with_the_builder_code() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(tmp.path());
    std::fs::write(app.join("src/main.crush"), "fn main() {\n    io.print(nope())\n}\n").unwrap();

    let out = crush_pkg(&app, &[]);
    let all = text(&out);
    assert!(!out.status.success());
    assert!(all.contains("crush-pkg[E-BUILDER]"), "{all}");
    assert!(!app.join("target/app.cvm").exists(), "failed build must not write target/");
}

fn python_capsule(root: &Path) -> std::path::PathBuf {
    let dir = root.join("py");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("capsule.toml"),
        "[capsule]\nname = \"py\"\nversion = \"0.1.0\"\nentry = \"main.py\"\nlanguage = \"python\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("main.py"), "print('hi')\n").unwrap();
    dir
}

#[test]
fn build_and_check_refuse_non_crush_capsules() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = python_capsule(tmp.path());

    for sub in ["build", "check"] {
        let out = crush_pkg(&dir, &[sub]);
        let all = text(&out);
        assert!(!out.status.success(), "{sub} should refuse a python capsule");
        assert!(
            all.contains(&format!("`crush-pkg {sub}` only applies to Crush-source capsules")),
            "{sub}: {all}"
        );
        assert!(all.contains("language = \"python\""), "{sub}: {all}");
        assert!(!dir.join("target").exists(), "{sub} wrote target/");
    }

    // Same refusal through the NDJSON wire.
    let out = crush_pkg(&dir, &["check", "--message-format", "json"]);
    let line = String::from_utf8_lossy(&out.stdout);
    assert!(line.contains("\"code\":\"E-BUILDER\""), "{line}");
    assert!(line.contains("only applies to Crush-source capsules"), "{line}");
}

#[cfg(unix)]
#[test]
fn bare_on_a_native_capsule_skips_build_and_passes_args() {
    let echo = Path::new("/bin/echo");
    if !echo.exists() {
        eprintln!("skipping: no /bin/echo");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("capsule.toml"),
        "[capsule]\nname = \"echoer\"\nversion = \"0.1.0\"\nentry = \"/bin/echo\"\nlanguage = \"native\"\n",
    )
    .unwrap();

    let out = crush_pkg(tmp.path(), &["--", "alpha", "beta"]);
    let all = text(&out);
    assert!(out.status.success(), "{all}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("alpha beta"), "{all}");
    assert!(!all.contains("building"), "native capsule must not be built:\n{all}");
    assert!(!tmp.path().join("target").exists());
}

#[test]
fn trailing_args_are_rejected_after_a_subcommand_other_than_run() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(tmp.path());
    let out = crush_pkg(&app, &["build", "stray"]);
    assert!(!out.status.success());
}

#[test]
fn show_prints_catalogue_fields_and_rejects_unknown_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_dep(tmp.path());
    let manifest = app.join("capsule.toml");
    let base = std::fs::read_to_string(&manifest).unwrap();
    let with = base.replace(
        "language = \"crush\"\n",
        "language = \"crush\"\ncategory = \"dev-tool\"\nplatforms = [\"linux\", \"web\"]\n",
    );
    std::fs::write(&manifest, &with).unwrap();
    let out = crush_pkg(&app, &["show"]);
    let all = text(&out);
    assert!(out.status.success(), "{all}");
    assert!(all.contains("category = \"dev-tool\""), "{all}");
    assert!(all.contains("platforms = ["), "{all}");

    std::fs::write(&manifest, with.replace("dev-tool", "toolz")).unwrap();
    let out = crush_pkg(&app, &["show"]);
    let all = text(&out);
    assert!(!out.status.success());
    assert!(all.contains("crush-pkg[E-MANIFEST]"), "{all}");
    assert!(all.contains("unknown [capsule] category \"toolz\""), "{all}");
}
