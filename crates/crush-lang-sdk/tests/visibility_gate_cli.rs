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
        ("src/app.crush", "fn main() { print(1) }\n"),
        (
            "src/script.crush",
            "fn helper() { return 1 }\nprint(helper())\n",
        ),
        (
            "src/demo.crush",
            "fn helper() { return 1 }\nfn main() { print(helper()) }\n",
        ),
    ]);
    let out = crushc(dir.path(), &["--list-exports", "src"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        "src/app.crush: program (exempt from the gate: nothing to export besides `main`)\n\
         src/demo.crush: legacy: no pub, everything public (its `fn main` is ignored when it is \
         imported)\n  public: helper\n\
         src/geo.crush: pub\n  pub: area\n  private: helper\n\
         src/old.crush: legacy: no pub, everything public\n  public: area, helper\n\
         src/script.crush: program (exempt from the gate: top-level statements, so it cannot be \
         imported)\n"
    );
    let gated = crushc(
        dir.path(),
        &["--list-exports", "--deny-legacy-visibility", "src"],
    );
    assert_eq!(gated.status.code(), Some(1), "{}", stderr(&gated));
    let err = stderr(&gated);
    assert!(err.contains("[E-VIS01] src/old.crush"), "{err}");
    assert!(
        err.contains("[E-VIS01] src/demo.crush"),
        "a library with a demo main is importable: {err}"
    );
    assert!(
        !err.contains("app.crush") && !err.contains("script.crush"),
        "programs are exempt: {err}"
    );
    assert!(
        err.contains("2 legacy module(s) fail the stage-2 gate"),
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

#[test]
fn list_exports_with_no_crush_files_passes_and_refuses_a_non_crush_file() {
    let dir = tree(&[("docs/notes.txt", "x")]);
    for args in [
        &["--list-exports", "docs"][..],
        &["--list-exports", "--deny-legacy-visibility", "docs"][..],
    ] {
        let out = crushc(dir.path(), args);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert!(stderr(&out).contains("no .crush files"), "{}", stderr(&out));
    }
    let out = crushc(dir.path(), &["--list-exports", "docs/notes.txt"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("is not a .crush file or a directory"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_gate_refuses_emit_ast() {
    let dir = tree(&[
        ("lib/geo.crush", LEGACY),
        (
            "main.crush",
            "import geo\nfn main() {\n    print(geo.area(1, 2))\n}\n",
        ),
    ]);
    let out = crushc(
        dir.path(),
        &[
            "--emit",
            "ast",
            "--deny-legacy-visibility",
            "-L",
            "lib",
            "main.crush",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("--emit ast"), "{}", stderr(&out));
    assert!(stdout(&out).is_empty(), "nothing emitted: {}", stdout(&out));
    let plain = crushc(dir.path(), &["--emit", "ast", "-L", "lib", "main.crush"]);
    assert!(
        plain.status.success(),
        "without the gate as before: {}",
        stderr(&plain)
    );
}

#[test]
fn e_vis01_json_level_is_error() {
    let dir = tree(&[
        ("lib/geo.crush", LEGACY),
        (
            "main.crush",
            "import geo\nfn main() {\n    print(geo.area(1, 2))\n}\n",
        ),
    ]);
    let out = crushc(
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
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let line = stderr(&out)
        .lines()
        .find(|l| l.contains("E-VIS01"))
        .unwrap_or_else(|| panic!("{}", stderr(&out)))
        .to_string();
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["code"], "E-VIS01");
    assert_eq!(v["level"], "error");
}

/// The directory walk follows what an import can follow (#545 review). The
/// resolver looks `import vendor.old` up as `<root>/vendor/old.crush` and
/// goes through a symlinked `vendor/`, so the walk follows it too.
#[cfg(unix)]
mod symlinked_dirs {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn a_legacy_module_behind_a_symlinked_directory_is_listed_and_fails_the_gate() {
        let dir = tree(&[
            ("src/geo.crush", PUB_GEO),
            ("shared/old.crush", LEGACY),
            (
                "main.crush",
                "import vendor.old\nfn main() {\n    print(old.area(2, 3))\n}\n",
            ),
        ]);
        symlink("../shared", dir.path().join("src/vendor")).unwrap();

        // The resolver reaches the module through the link ...
        let check = crushc(dir.path(), &["--check", "-L", "src", "main.crush"]);
        assert!(check.status.success(), "{}", stderr(&check));

        // ... so the listing shows it, under the path an import would use ...
        let out = crushc(dir.path(), &["--list-exports", "src"]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(
            stdout(&out),
            "src/geo.crush: pub\n  pub: area\n  private: helper\n\
             src/vendor/old.crush: legacy: no pub, everything public\n  public: area, helper\n"
        );
        // ... and the directory gate fails it.
        let gated = crushc(
            dir.path(),
            &["--list-exports", "--deny-legacy-visibility", "src"],
        );
        assert_eq!(gated.status.code(), Some(1), "{}", stderr(&gated));
        let err = stderr(&gated);
        assert!(err.contains("[E-VIS01] src/vendor/old.crush"), "{err}");
        assert!(
            err.contains("1 legacy module(s) fail the stage-2 gate"),
            "{err}"
        );
    }

    #[test]
    fn a_symlink_loop_terminates_and_lists_each_directory_once() {
        let dir = tree(&[("src/geo.crush", PUB_GEO), ("src/sub/old.crush", LEGACY)]);
        let src = dir.path().join("src");
        // A link to the directory it sits in, a link back up from below, and
        // two links that point at each other (they resolve to nothing).
        symlink(".", src.join("self")).unwrap();
        symlink("..", src.join("sub/up")).unwrap();
        symlink("pong", src.join("ping")).unwrap();
        symlink("ping", src.join("pong")).unwrap();
        // The same directory again, beside the real one: listed once, under
        // the real path.
        symlink("sub", src.join("alias")).unwrap();

        let out = crushc(dir.path(), &["--list-exports", "src"]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(
            stdout(&out),
            "src/geo.crush: pub\n  pub: area\n  private: helper\n\
             src/sub/old.crush: legacy: no pub, everything public\n  public: area, helper\n"
        );
        let gated = crushc(
            dir.path(),
            &["--list-exports", "--deny-legacy-visibility", "src"],
        );
        assert_eq!(gated.status.code(), Some(1), "{}", stderr(&gated));
        assert!(
            stderr(&gated).contains("1 legacy module(s) fail the stage-2 gate"),
            "{}",
            stderr(&gated)
        );
    }

    /// A hidden directory stays out of the walk because no import can reach
    /// it: the parser takes only identifiers in a module path, and none
    /// starts with `.`, so the resolver is never asked. That holds for a
    /// real one and for a link.
    #[test]
    fn a_hidden_directory_is_skipped_because_no_import_can_name_it() {
        let dir = tree(&[
            ("src/geo.crush", PUB_GEO),
            ("src/.cache/old.crush", LEGACY),
            ("shared/old.crush", LEGACY),
            (
                "main.crush",
                "import .cache.old\nfn main() {\n    print(1)\n}\n",
            ),
        ]);
        symlink("../shared", dir.path().join("src/.vendor")).unwrap();

        let check = crushc(dir.path(), &["--check", "-L", "src", "main.crush"]);
        assert!(
            !check.status.success(),
            "`import .cache.old` must not resolve: {}",
            stderr(&check)
        );

        let gated = crushc(
            dir.path(),
            &["--list-exports", "--deny-legacy-visibility", "src"],
        );
        assert!(gated.status.success(), "{}", stderr(&gated));
        assert_eq!(
            stdout(&gated),
            "src/geo.crush: pub\n  pub: area\n  private: helper\n"
        );

        // Named as the root, it is walked: a hidden directory can be a search
        // root (`-L src/.cache`), and the root is never filtered.
        let root = crushc(
            dir.path(),
            &["--list-exports", "--deny-legacy-visibility", "src/.cache"],
        );
        assert_eq!(root.status.code(), Some(1), "{}", stderr(&root));
        assert!(
            stderr(&root).contains("[E-VIS01] src/.cache/old.crush"),
            "{}",
            stderr(&root)
        );
    }
}
