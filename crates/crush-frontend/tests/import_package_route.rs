//! Packages and symlinks (#552). A file's package comes from where it
//! really is (its canonical path), when that is inside a known root: the
//! file decides, whatever route reached it. A file that is really under
//! **no** root takes the package of the path it was **found through**: the
//! canonical directory that resolved the import (a `-L` directory, a package
//! root, or the importing file's own found directory), then the module's
//! segments as written. So a module reached through a link that leaves its
//! `-L` root for a directory under no root is in that root's package, and
//! visibility findings about it are reported exactly as for the same files
//! in a real directory. Only when two packages each hold a link to a file
//! under no root does the route matter: the first import that loads it
//! decides.
#![cfg(unix)]

use std::path::PathBuf;

use crush_cast::Program;
use crush_frontend::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_frontend::import_system::{E_VIS01, ImportOptions, ImportPackage, N_VIS01, W_VIS01};

/// A fresh directory under the system temp dir holding `files`, then the
/// symlinks `links` (`(link, target)`, the target as the link stores it).
fn tree(tag: &str, files: &[(&str, &str)], links: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "crush-import-package-route-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    for (link, target) in links {
        let path = dir.join(link);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, path).unwrap();
    }
    dir
}

/// Compile `<tree>/app/main.crush` with `-L <tree>/<lib>`, with or without
/// the stage-2 gate; the program and its visibility findings.
fn compile(
    tag: &str,
    files: &[(&str, &str)],
    links: &[(&str, &str)],
    lib: &str,
    gate: bool,
) -> (Program, Vec<CompilerDiagnostic>) {
    let dir = tree(tag, files, links);
    let mut opts = ImportOptions::new([dir.join(lib)]);
    opts.deny_legacy_visibility = gate;
    let r = crush_frontend::parse_file_reporting(&dir.join("app/main.crush"), &opts)
        .unwrap_or_else(|e| panic!("{tag}: the compile must succeed: {e:#}"));
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn codes(d: &[CompilerDiagnostic]) -> Vec<&'static str> {
    d.iter().map(|d| d.code).collect()
}

/// Whether every finding is an error (`true`) or none is (`false`).
fn all_errors(d: &[CompilerDiagnostic]) -> bool {
    d.iter().all(|d| d.severity == DiagnosticSeverity::Error)
}

fn package(program: &Program, function: &str) -> String {
    program.functions[function].meta["package"]
        .as_str()
        .unwrap()
        .to_string()
}

const GEO: &str = "pub fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\n";
const OLD: &str = "fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\n";

/// `main.crush` importing `<dir>.geo` and calling its `helper`.
fn main_using(dir: &str) -> String {
    format!("import {dir}.geo\nfn main() {{\n    print(geo.helper(1))\n}}\n")
}

/// A fixture: its name, its files, and its symlinks (`(link, target)`).
type Layout = (
    &'static str,
    Vec<(&'static str, String)>,
    Vec<(&'static str, &'static str)>,
);

/// The two layouts every case is run on: `geo.crush` in a directory outside
/// the `-L` root, reached through the symlink `lib/vendor -> ../outside`; and
/// the control, the same file in the real directory `lib/real`.
fn layouts(geo: &'static str) -> [Layout; 2] {
    [
        (
            "symlink",
            vec![
                ("outside/geo.crush", geo.to_string()),
                ("lib/keep.crush", "fn k() { return 0 }\n".to_string()),
                ("app/main.crush", main_using("vendor")),
            ],
            vec![("lib/vendor", "../outside")],
        ),
        (
            "real",
            vec![
                ("lib/real/geo.crush", geo.to_string()),
                ("app/main.crush", main_using("real")),
            ],
            vec![],
        ),
    ]
}

fn borrowed<'a>(files: &'a [(&'static str, String)]) -> Vec<(&'a str, &'a str)> {
    files.iter().map(|(p, s)| (*p, s.as_str())).collect()
}

#[test]
fn a_private_fn_through_a_symlink_that_leaves_the_search_root_warns_like_a_real_directory() {
    // Finding dc26654c, case 1. By its canonical path `outside/geo.crush` is
    // under no root, so it used to fall into the root package and the call of
    // its private `helper` from `app/` was silent.
    for (layout, files, links) in layouts(GEO) {
        let dir = if layout == "symlink" {
            "vendor"
        } else {
            "real"
        };
        for gate in [false, true] {
            let tag = format!("priv-{layout}-{gate}");
            let (program, d) = compile(&tag, &borrowed(&files), &links, "lib", gate);
            assert_eq!(codes(&d), [W_VIS01], "{tag}: {d:?}");
            let want = if gate {
                DiagnosticSeverity::Error
            } else {
                DiagnosticSeverity::Warning
            };
            assert!(d[0].severity == want, "{tag}: {d:?}");
            assert!(d[0].location.file.ends_with("main.crush"), "{tag}: {d:?}");
            let pkg = package(&program, &format!("{dir}::geo::helper"));
            assert!(
                pkg.starts_with("dir:") && pkg.ends_with("lib"),
                "{tag}: the module is in the `-L lib` package, got {pkg}"
            );
        }
    }
}

#[test]
fn a_legacy_module_through_a_symlink_that_leaves_the_search_root_fails_the_gate_like_a_real_directory()
 {
    // Finding dc26654c, case 2: `--deny-legacy-visibility` passed (rc 0) a
    // legacy module reached this way.
    for (layout, files, links) in layouts(OLD) {
        let tag = format!("legacy-{layout}");
        let (_, d) = compile(&tag, &borrowed(&files), &links, "lib", true);
        assert_eq!(codes(&d), [E_VIS01], "{tag}: {d:?}");
        assert!(all_errors(&d), "{tag}: {d:?}");

        let tag = format!("legacy-note-{layout}");
        let (_, d) = compile(&tag, &borrowed(&files), &links, "lib", false);
        assert_eq!(
            codes(&d),
            [N_VIS01],
            "{tag}: without the gate, V1's note: {d:?}"
        );
    }
}

#[test]
fn a_symlinked_file_is_in_the_package_of_the_directory_holding_the_link() {
    let (program, d) = compile(
        "file-link",
        &[
            ("outside/geo.crush", GEO),
            (
                "app/main.crush",
                "import geo\nfn main() {\n    print(geo.helper(1))\n}\n",
            ),
        ],
        &[("lib/geo.crush", "../outside/geo.crush")],
        "lib",
        false,
    );
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    let pkg = package(&program, "geo::helper");
    assert!(pkg.starts_with("dir:") && pkg.ends_with("lib"), "{pkg}");
}

#[test]
fn a_module_beside_a_symlinked_module_shares_its_package() {
    // `geo` (reached through the link) imports its sibling `util` relatively:
    // the route continues the importer's, so both are in the `lib` package
    // and `geo` may use `util`'s private name. `app/` may not.
    let (program, d) = compile(
        "sibling",
        &[
            (
                "outside/util.crush",
                "pub fn shown() { return 1 }\nfn inner() { return 2 }\n",
            ),
            (
                "outside/geo.crush",
                "import util\npub fn area(w, h) { return w * h + util.inner() }\n",
            ),
            (
                "app/main.crush",
                "import vendor.geo\nimport vendor.util\nfn main() {\n    print(geo.area(1, 2))\n    print(util.inner())\n}\n",
            ),
        ],
        &[("lib/vendor", "../outside")],
        "lib",
        false,
    );
    assert_eq!(codes(&d), [W_VIS01], "only the use from app/: {d:?}");
    assert!(d[0].location.file.ends_with("main.crush"), "{d:?}");
    assert_eq!(
        package(&program, "util::inner"),
        package(&program, "vendor::geo::area")
    );
    let inner: Vec<_> = program
        .functions
        .keys()
        .filter(|n| n.ends_with("::inner"))
        .collect();
    assert_eq!(inner.len(), 1, "util is loaded once: {inner:?}");
}

#[test]
fn a_search_directory_that_is_itself_a_symlink_is_still_one_canonical_root() {
    // Only symlinks *below* a root are kept as written. The root itself is
    // canonical, so `-L liblink` (-> lib) and the relative route
    // `import lib.geo` name the same package, as before.
    let files = [
        ("app/lib/geo.crush", GEO),
        (
            "app/main.crush",
            "import lib.geo\nfn main() {\n    print(geo.helper(1))\n}\n",
        ),
    ];
    let (program, d) = compile(
        "root-link",
        &files,
        &[("liblink", "app/lib")],
        "liblink",
        false,
    );
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    let pkg = package(&program, "lib::geo::helper");
    assert!(pkg.starts_with("dir:") && pkg.ends_with("liblink"), "{pkg}");
}

/// `main.crush` with the two imports in the given order, using `helper`
/// through both aliases `a` and `b`.
fn main_importing(first: &str, second: &str) -> String {
    format!(
        "import {first}\nimport {second}\nfn main() {{\n    print(a.helper(1))\n    print(b.helper(1))\n}}\n"
    )
}

fn helpers(p: &Program) -> Vec<String> {
    p.functions
        .keys()
        .filter(|n| n.ends_with("::helper"))
        .cloned()
        .collect()
}

#[test]
fn a_file_inside_a_root_keeps_its_package_when_another_package_links_to_it() {
    // `lib/real/geo.crush` really is in `-L lib`. The root package also
    // reaches it through its own link `app/mine -> ../lib/real`. Where the
    // file is decides, so it is the library's in either import order, and
    // both uses from `app/` warn, the one through the app's link included.
    for (tag, first, second) in [
        ("own-first", "mine.geo as a", "real.geo as b"),
        ("lib-first", "real.geo as b", "mine.geo as a"),
    ] {
        let main = main_importing(first, second);
        let (program, d) = compile(
            &format!("two-routes-{tag}"),
            &[("lib/real/geo.crush", GEO), ("app/main.crush", &main)],
            &[("app/mine", "../lib/real")],
            "lib",
            false,
        );
        assert_eq!(codes(&d), [W_VIS01, W_VIS01], "{tag}: {d:?}");
        let h = helpers(&program);
        assert_eq!(h.len(), 1, "{tag}: loaded once: {h:?}");
        let pkg = package(&program, &h[0]);
        assert!(
            pkg.starts_with("dir:") && pkg.ends_with("lib"),
            "{tag}: {pkg}"
        );
    }
}

#[test]
fn a_library_stays_clean_when_the_app_imports_its_file_through_a_link_first() {
    // The review's case: the app imports `geo` through its own link before
    // the library's `user` imports its sibling `geo`. The library's own use
    // of `geo.helper` must stay same-package (silent, and clean under the
    // gate), whichever import comes first.
    for (tag, imports) in [
        ("link-first", "import mine.geo as a\nimport real.user"),
        ("lib-first", "import real.user\nimport mine.geo as a"),
    ] {
        let main =
            format!("{imports}\nfn main() {{\n    print(user.go())\n    print(a.area(1, 2))\n}}\n");
        for gate in [false, true] {
            let tag = format!("lib-clean-{tag}-{gate}");
            let (program, d) = compile(
                &tag,
                &[
                    ("lib/real/geo.crush", GEO),
                    (
                        "lib/real/user.crush",
                        "import geo\npub fn go() { return geo.helper(1) }\n",
                    ),
                    ("app/main.crush", &main),
                ],
                &[("app/mine", "../lib/real")],
                "lib",
                gate,
            );
            assert!(d.is_empty(), "{tag}: {d:?}");
            assert_eq!(
                package(&program, "real::user::go"),
                package(&program, &helpers(&program)[0]),
                "{tag}"
            );
        }
    }
}

#[test]
fn a_file_under_no_root_linked_from_two_packages_takes_the_first_import_s_package() {
    // The one case where the route decides between packages:
    // `outside/geo.crush` is under no root, and both `-L lib`
    // (`lib/vendor -> ../outside`) and the root package
    // (`app/mine -> ../outside`) hold a link to it. It is loaded once, so it
    // has one package: that of the first import that loads it. Imports
    // resolve in source order, depth first, so this is the same on every
    // compile.
    let fixture = |tag: &str, first: &str, second: &str| {
        let main = main_importing(first, second);
        compile(
            tag,
            &[("outside/geo.crush", GEO), ("app/main.crush", &main)],
            &[("lib/vendor", "../outside"), ("app/mine", "../outside")],
            "lib",
            false,
        )
    };

    // The root package's link first: the module is the root package's, and
    // both uses are same-package.
    let (program, d) = fixture("no-root-own-first", "mine.geo as a", "vendor.geo as b");
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(helpers(&program), ["mine::geo::helper"], "loaded once");
    assert_eq!(package(&program, "mine::geo::helper"), "root");

    // The `-L` route first: the module is the library's, and both uses warn.
    let (program, d) = fixture("no-root-lib-first", "vendor.geo as b", "mine.geo as a");
    assert_eq!(codes(&d), [W_VIS01, W_VIS01], "{d:?}");
    assert_eq!(helpers(&program), ["vendor::geo::helper"], "loaded once");
    let pkg = package(&program, "vendor::geo::helper");
    assert!(pkg.starts_with("dir:") && pkg.ends_with("lib"), "{pkg}");
}

#[test]
fn a_link_below_a_named_package_root_keeps_the_module_in_that_package() {
    // The package-name route: `import facet.sub.geo` resolves inside the
    // package root, where `sub -> ../outside` leaves it for a directory under
    // no root. The module is in `pkg:facet`, so the root package's use of its
    // private name warns; so does the control with `sub` a real directory.
    for (tag, geo_at, links) in [
        (
            "link",
            "outside/geo.crush",
            vec![("facet/sub", "../outside")],
        ),
        ("real", "facet/sub/geo.crush", vec![]),
    ] {
        let dir = tree(
            &format!("pkg-{tag}"),
            &[
                (geo_at, GEO),
                ("facet/facet.crush", "pub fn f() { return 1 }\n"),
                (
                    "app/main.crush",
                    "import facet.sub.geo\nfn main() {\n    print(geo.helper(1))\n}\n",
                ),
            ],
            &links,
        );
        let opts = ImportOptions {
            packages: vec![ImportPackage {
                name: "facet".into(),
                root: dir.join("facet"),
                entry: dir.join("facet/facet.crush"),
            }],
            ..Default::default()
        };
        let (program, d) = crush_frontend::parse_file_reporting(&dir.join("app/main.crush"), &opts)
            .unwrap_or_else(|e| panic!("{tag}: the compile must succeed: {e:#}"));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(codes(&d), [W_VIS01], "{tag}: {d:?}");
        assert_eq!(
            package(&program, "facet::sub::geo::helper"),
            "pkg:facet",
            "{tag}"
        );
    }
}

#[test]
fn a_symlinked_entry_file_keeps_the_package_of_where_it_really_is() {
    // Unchanged from before #552: the entry `app/main.crush` is a link to
    // `lib/entry.crush`, which really is in `-L lib`, so it may use the
    // private names of its real neighbours.
    let (_, d) = compile(
        "entry-link",
        &[
            ("lib/geo.crush", GEO),
            (
                "lib/entry.crush",
                "import geo\nfn main() {\n    print(geo.helper(1))\n}\n",
            ),
        ],
        &[("app/main.crush", "../lib/entry.crush")],
        "lib",
        false,
    );
    assert!(d.is_empty(), "{d:?}");
}
