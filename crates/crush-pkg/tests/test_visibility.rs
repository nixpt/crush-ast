//! Visibility findings in package builds (#539): `crush-pkg build` and the
//! test compile (`PackageBuilder::build_file`) print `W-VIS01`, `N-VIS01` and
//! `W-VIS02` and never fail on them; `crush-pkg build --deny-legacy-visibility`
//! is the stage-2 gate.

use std::path::Path;
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

fn crush_pkg(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-pkg"))
        .args(args)
        .current_dir(dir)
        .env("CRUSH_PKG_CACHE", dir.join(".cache"))
        .env_remove("CRUSH_PKG_OFFLINE")
        .env_remove("CRUSH_PATH")
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-pkg")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), stderr(o))
}

const LIB_TOML: &str = "[capsule]\nname = \"geo\"\nversion = \"0.1.0\"\nentry = \"src/geo.crush\"\nlanguage = \"crush\"\ncategory = \"library\"\n";

const APP_TOML: &str = "[capsule]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n\n[[dependencies]]\nname = \"geo\"\npath = \"../geo\"\n";

/// `<tmp>/geo` (a library whose entry is `geo_src`) and `<tmp>/app`, whose
/// `src/main.crush` is `main_src`; the app's directory.
fn app_with_lib(tmp: &Path, geo_src: &str, main_src: &str) -> std::path::PathBuf {
    write(
        &tmp.join("geo"),
        &[("capsule.toml", LIB_TOML), ("src/geo.crush", geo_src)],
    );
    let app = tmp.join("app");
    write(
        &app,
        &[("capsule.toml", APP_TOML), ("src/main.crush", main_src)],
    );
    app
}

const LEGACY_GEO: &str = "fn area(w, h) {\n    return w * h\n}\n";
const PUB_GEO: &str =
    "pub fn area(w, h) {\n    return w * h\n}\nfn helper(x) {\n    return x + 1\n}\n";

#[test]
fn build_prints_the_legacy_note_and_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_lib(
        tmp.path(),
        LEGACY_GEO,
        "import geo\n\nfn main() {\n    io.print(geo.area(2, 3))\n}\n",
    );
    let out = crush_pkg(&app, &["build"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(stderr(&out).contains("[N-VIS01]"), "{}", text(&out));
}

#[test]
fn build_prints_w_vis01_and_succeeds_and_the_gate_fails_it() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_lib(
        tmp.path(),
        PUB_GEO,
        "import geo\n\nfn main() {\n    io.print(geo.area(2, 3))\n    io.print(geo.helper(1))\n}\n",
    );
    let out = crush_pkg(&app, &["build"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        stderr(&out).contains("warning") && stderr(&out).contains("[W-VIS01]"),
        "{}",
        text(&out)
    );
    let gated = crush_pkg(&app, &["build", "--deny-legacy-visibility"]);
    assert!(!gated.status.success(), "{}", text(&gated));
    assert!(stderr(&gated).contains("[W-VIS01]"), "{}", text(&gated));
    assert!(
        text(&gated).contains("fail the stage-2 gate"),
        "{}",
        text(&gated)
    );
}

#[test]
fn the_gate_fails_a_legacy_dependency_the_app_imports() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_lib(
        tmp.path(),
        LEGACY_GEO,
        "import geo\n\nfn main() {\n    io.print(geo.area(2, 3))\n}\n",
    );
    let out = crush_pkg(&app, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(stderr(&out).contains("[E-VIS01]"), "{}", text(&out));
    assert!(!stderr(&out).contains("[N-VIS01]"), "{}", text(&out));
}

#[test]
fn the_gate_fails_a_library_with_a_legacy_module_nobody_imports() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("my-lib");
    crush_pkg::manifest::scaffold_lib_package(&lib, "my-lib").unwrap();
    let plain = crush_pkg(&lib, &["build"]);
    assert!(
        plain.status.success(),
        "no gate, no failure: {}",
        text(&plain)
    );
    let out = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    let err = stderr(&out);
    assert!(
        err.contains("lib.crush: legacy: no pub, everything public") && err.contains("greet"),
        "lists the module and its names: {err}"
    );
    assert!(err.contains("my-lib"), "names the package: {err}");
    // Adding `pub` passes the gate.
    write(
        &lib,
        &[(
            "src/lib.crush",
            "pub fn greet(who) {\n    return \"hello, \" + who\n}\n",
        )],
    );
    let ok = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(ok.status.success(), "{}", text(&ok));
}

#[test]
fn the_package_gate_checks_the_entry_like_any_module() {
    // A library gate: the entry is checked too. A main-only entry passes;
    // an entry with helpers is importable and fails.
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("app");
    write(
        &app,
        &[
            (
                "capsule.toml",
                "[capsule]\nname = \"app\"\nversion = \"0.1.0\"\nentry = \"src/main.crush\"\nlanguage = \"crush\"\n",
            ),
            (
                "src/main.crush",
                "import util\n\nfn main() {\n    io.print(util.hi())\n}\n",
            ),
            ("src/util.crush", "pub fn hi() {\n    return \"hi\"\n}\n"),
        ],
    );
    let out = crush_pkg(&app, &["build", "--deny-legacy-visibility"]);
    assert!(out.status.success(), "main-only entry: {}", text(&out));
    write(
        &app,
        &[(
            "src/main.crush",
            "import util\n\nfn shout(x) {\n    return x + \"!\"\n}\n\nfn main() {\n    io.print(shout(util.hi()))\n}\n",
        )],
    );
    let out = crush_pkg(&app, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "entry with helpers: {}", text(&out));
    assert!(stderr(&out).contains("main.crush"), "{}", text(&out));
    // Without the gate it builds as before.
    let plain = crush_pkg(&app, &["build"]);
    assert!(plain.status.success(), "{}", text(&plain));
}

#[test]
fn an_uncategorised_library_entry_with_exported_functions_fails_the_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("geo");
    write(
        &lib,
        &[
            (
                "capsule.toml",
                "[capsule]\nname = \"geo\"\nversion = \"0.1.0\"\nentry = \"src/geo.crush\"\nlanguage = \"crush\"\n",
            ),
            ("src/geo.crush", LEGACY_GEO),
        ],
    );
    let out = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        stderr(&out).contains("geo.crush: legacy: no pub, everything public"),
        "{}",
        text(&out)
    );
}

#[test]
fn the_test_compile_carries_and_prints_the_findings() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_lib(
        tmp.path(),
        "pub fn area(w, h) {\n    return w * h\n}\npub fn __raw(x) {\n    return x\n}\nfn helper(x) {\n    return x + 1\n}\n",
        "import geo\n\nfn main() {\n    io.print(geo.area(2, 3))\n}\n",
    );
    write(
        &app,
        &[(
            "tests/t.crush",
            "import geo\n\nfn main() {\n    io.print(geo.helper(1))\n    io.print(geo.__raw(1))\n}\n",
        )],
    );
    let m = Manifest::from_file(&app.join("capsule.toml")).unwrap();
    let out = PackageBuilder::new(m, app.clone())
        .quiet(true)
        .build_file(&app.join("tests/t.crush"))
        .unwrap();
    let codes: Vec<&str> = out.visibility.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["W-VIS01", "W-VIS02"], "{:?}", out.visibility);
}

#[test]
fn the_gate_checks_only_source_modules_when_the_entry_is_at_the_root() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("rootpkg");
    write(
        &pkg,
        &[
            (
                "capsule.toml",
                "[capsule]\nname = \"rootpkg\"\nversion = \"0.1.0\"\nentry = \"main.crush\"\nlanguage = \"crush\"\n",
            ),
            (
                "main.crush",
                "import util\n\nfn main() {\n    io.print(util.hi())\n}\n",
            ),
            ("util.crush", "pub fn hi() {\n    return \"hi\"\n}\n"),
            // Legacy and importable, but not package sources.
            ("tests/helpers.crush", "fn fixture() {\n    return 1\n}\n"),
            ("examples/demo.crush", "fn show() {\n    return 2\n}\n"),
        ],
    );
    let out = crush_pkg(&pkg, &["build", "--deny-legacy-visibility"]);
    assert!(out.status.success(), "{}", text(&out));
    // A legacy source module at the root still fails.
    write(&pkg, &[("extra.crush", "fn more() {\n    return 3\n}\n")]);
    let out = crush_pkg(&pkg, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    let err = stderr(&out);
    assert!(err.contains("extra.crush"), "{err}");
    assert!(
        !err.contains("helpers.crush") && !err.contains("demo.crush"),
        "{err}"
    );
}

#[test]
fn the_gate_fails_a_library_whose_entry_has_a_demo_main() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("geo");
    write(
        &lib,
        &[
            ("capsule.toml", LIB_TOML),
            (
                "src/geo.crush",
                "fn area(w, h) {\n    return w * h\n}\n\nfn main() {\n    io.print(area(2, 3))\n}\n",
            ),
        ],
    );
    let out = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(stderr(&out).contains("geo.crush"), "{}", text(&out));
}

#[test]
fn the_gate_fails_an_all_legacy_concatenation_mode_package() {
    // A library with no `import` in its entry builds by concatenation; its
    // modules are checked like any package's.
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("geo");
    write(
        &lib,
        &[
            ("capsule.toml", LIB_TOML),
            ("src/geo.crush", LEGACY_GEO),
            ("src/more.crush", "fn helper() {\n    return 1\n}\n"),
        ],
    );
    let plain = crush_pkg(&lib, &["build"]);
    assert!(plain.status.success(), "{}", text(&plain));
    let out = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    let err = stderr(&out);
    assert!(
        err.contains("geo.crush") && err.contains("more.crush"),
        "{err}"
    );

    // An app that reaches a dependency by concatenation (no `import`) relies
    // on all of the dependency's names being public: refused under the gate.
    let app = tmp.path().join("app");
    write(
        &app,
        &[
            ("capsule.toml", APP_TOML),
            (
                "src/main.crush",
                "fn main() {\n    io.print(area(2, 3))\n}\n",
            ),
        ],
    );
    let plain = crush_pkg(&app, &["build"]);
    assert!(plain.status.success(), "{}", text(&plain));
    let out = crush_pkg(&app, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("by concatenation"), "{}", text(&out));
}

#[test]
fn json_mode_keeps_the_findings_off_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_lib(
        tmp.path(),
        PUB_GEO,
        "import geo\n\nfn main() {\n    io.print(geo.helper(1))\n}\n",
    );
    let out = crush_pkg(&app, &["--message-format", "json", "build"]);
    assert!(out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("VIS0"),
        "findings are not on stdout: {stdout}"
    );
    assert!(stderr(&out).contains("[W-VIS01]"), "{}", text(&out));
}

#[test]
fn json_mode_keeps_the_gate_errors_off_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_lib(
        tmp.path(),
        LEGACY_GEO,
        "import geo\n\nfn main() {\n    io.print(geo.area(2, 3))\n}\n",
    );
    let out = crush_pkg(
        &app,
        &[
            "--message-format",
            "json",
            "build",
            "--deny-legacy-visibility",
        ],
    );
    assert!(!out.status.success(), "{}", text(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("VIS0"),
        "findings are not on stdout: {stdout}"
    );
    assert!(stderr(&out).contains("[E-VIS01]"), "{}", text(&out));
}

/// The package gate walks what the resolver sees (#545 review): an import
/// goes through a symlinked `src/vendor/`, so a legacy module behind the
/// link fails the gate like one in a real directory.
#[cfg(unix)]
#[test]
fn the_package_gate_follows_a_symlinked_source_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = tmp.path().join("geo");
    write(
        &lib,
        &[("capsule.toml", LIB_TOML), ("src/geo.crush", PUB_GEO)],
    );
    write(tmp.path(), &[("shared/old.crush", LEGACY_GEO)]);
    let ok = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(ok.status.success(), "no link yet: {}", text(&ok));

    std::os::unix::fs::symlink("../../shared", lib.join("src/vendor")).unwrap();
    // A loop beside it does not hang the walk.
    std::os::unix::fs::symlink("..", lib.join("src/up")).unwrap();
    let out = crush_pkg(&lib, &["build", "--deny-legacy-visibility"]);
    assert!(!out.status.success(), "{}", text(&out));
    let err = stderr(&out);
    assert!(
        err.contains("vendor/old.crush: legacy: no pub, everything public") && err.contains("area"),
        "lists the module behind the link: {err}"
    );
    assert!(
        err.contains("1 module(s) of package `geo`"),
        "and only that one: {err}"
    );
}
