//! `crushc --deny-legacy-visibility` and `crushc --list-exports` (#539), run
//! through the real binary with stdin closed.

use std::path::Path;
use std::process::{Command, Output, Stdio};

fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, src) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

fn crushc(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crushc"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .env_remove("CRUSH_PATH")
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

const LEGACY: &str = "fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\n";
const PUB_GEO: &str = "pub fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\n";

#[test]
fn gate_fails_a_legacy_module_used_across_packages_and_without_it_the_output_is_v1s() {
    let dir = tree(&[
        ("lib/geo.crush", LEGACY),
        (
            "main.crush",
            "import geo\nfn main() {\n    print(geo.area(2, 3))\n    print(geo.helper(1))\n}\n",
        ),
    ]);
    let plain = crushc(dir.path(), &["--check", "-L", "lib", "main.crush"]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    assert!(stderr(&plain).contains("[N-VIS01]"), "{}", stderr(&plain));
    assert!(!stderr(&plain).contains("E-VIS01"), "{}", stderr(&plain));
    // --deny-warnings ignores the note, as on main.
    let deny = crushc(
        dir.path(),
        &["--check", "--deny-warnings", "-L", "lib", "main.crush"],
    );
    assert!(deny.status.success(), "{}", stderr(&deny));

    let gated = crushc(
        dir.path(),
        &[
            "--check",
            "--deny-legacy-visibility",
            "-L",
            "lib",
            "main.crush",
        ],
    );
    assert_eq!(gated.status.code(), Some(1), "{}", stderr(&gated));
    let err = stderr(&gated);
    assert_eq!(
        err.matches("[E-VIS01]").count(),
        1,
        "once per module: {err}"
    );
    assert!(!err.contains("[N-VIS01]"), "{err}");
    assert!(err.contains("uses it: area, helper "), "{err}");
    assert!(
        err.contains("1 visibility finding(s) fail the stage-2 gate"),
        "{err}"
    );
}

#[test]
fn gate_fails_w_vis01_and_its_json_level_is_error() {
    let dir = tree(&[
        ("lib/geo.crush", PUB_GEO),
        (
            "main.crush",
            "import geo\nfn main() {\n    print(geo.helper(1))\n}\n",
        ),
    ]);
    let plain = crushc(
        dir.path(),
        &[
            "--check",
            "--message-format",
            "json",
            "-L",
            "lib",
            "main.crush",
        ],
    );
    assert!(plain.status.success(), "{}", stderr(&plain));
    assert!(
        stderr(&plain).contains("\"code\":\"W-VIS01\"")
            && stderr(&plain).contains("\"level\":\"warning\""),
        "{}",
        stderr(&plain)
    );
    let gated = crushc(
        dir.path(),
        &[
            "--check",
            "--message-format",
            "json",
            "--deny-legacy-visibility",
            "-L",
            "lib",
            "main.crush",
        ],
    );
    assert_eq!(gated.status.code(), Some(1), "{}", stderr(&gated));
    assert!(
        stderr(&gated).contains("\"code\":\"W-VIS01\"")
            && stderr(&gated).contains("\"level\":\"error\""),
        "{}",
        stderr(&gated)
    );
}

#[test]
fn gate_passes_a_pub_library_and_a_single_package_program() {
    let dir = tree(&[
        ("lib/geo.crush", PUB_GEO),
        ("util.crush", LEGACY),
        (
            "main.crush",
            "import geo\nimport util\nfn main() {\n    print(geo.area(2, 3))\n    print(util.helper(1))\n}\n",
        ),
    ]);
    let out = crushc(
        dir.path(),
        &[
            "--check",
            "--deny-legacy-visibility",
            "-L",
            "lib",
            "main.crush",
        ],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stderr(&out).contains("VIS0"), "{}", stderr(&out));
}

#[test]
fn list_exports_shows_each_modules_surface_and_the_gate_fails_legacy_ones() {
    let dir = tree(&[
        ("src/geo.crush", PUB_GEO),
        ("src/old.crush", LEGACY),
        (
            "src/app.crush",
            "fn helper() { return 1 }\nfn main() { print(helper()) }\n",
        ),
    ]);
    let out = crushc(dir.path(), &["--list-exports", "src"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "src/app.crush: legacy: no pub, everything public (a program: not checked by the gate \
         unless another package imports it)\n  public: helper\n\
         src/geo.crush: pub\n  pub: area\n  private: helper\n\
         src/old.crush: legacy: no pub, everything public\n  public: area, helper\n"
    );
    let gated = crushc(
        dir.path(),
        &["--list-exports", "--deny-legacy-visibility", "src"],
    );
    assert_eq!(gated.status.code(), Some(1), "{}", stderr(&gated));
    let err = stderr(&gated);
    assert!(err.contains("[E-VIS01] src/old.crush"), "{err}");
    assert!(
        !err.contains("app.crush"),
        "a program is not refused: {err}"
    );
    assert!(
        err.contains("1 legacy module(s) fail the stage-2 gate"),
        "{err}"
    );

    // One file works too, and a pub module passes.
    let one = crushc(
        dir.path(),
        &[
            "--list-exports",
            "--deny-legacy-visibility",
            "src/geo.crush",
        ],
    );
    assert!(one.status.success(), "{}", stderr(&one));
    assert_eq!(
        stdout(&one),
        "src/geo.crush: pub\n  pub: area\n  private: helper\n"
    );
}

#[test]
fn list_exports_fails_on_a_file_that_does_not_parse() {
    let dir = tree(&[("src/bad.crush", "fn ( {")]);
    let out = crushc(dir.path(), &["--list-exports", "src"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).contains("src/bad.crush"), "{}", stderr(&out));
}
