//! The visibility stage-2 gate (V1.5, #539): under
//! `ImportOptions::deny_legacy_visibility` a `W-VIS01` is an error and a
//! legacy module (no `pub` anywhere) that another package uses gets one
//! `E-VIS01` error naming every name used; without it the findings are V1's.
//! Also the library author's view, `visibility_check::module_surface`, which
//! needs no foreign user.

use std::path::{Path, PathBuf};

use crush_frontend::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_frontend::import_system::{E_VIS01, ImportOptions, N_VIS01, W_VIS01};
use crush_frontend::visibility_check::{module_surface, render_surface, surfaces};

fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "crush-visibility-gate-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

/// Compile `<tree>/main.crush` with `-L <tree>/lib`; the findings.
fn compile(tag: &str, files: &[(&str, &str)], gate: bool) -> Vec<CompilerDiagnostic> {
    let dir = tree(tag, files);
    let mut opts = ImportOptions::new([dir.join("lib")]);
    opts.deny_legacy_visibility = gate;
    let r = crush_frontend::parse_file_reporting(&dir.join("main.crush"), &opts)
        .unwrap_or_else(|e| panic!("{tag}: the compile must succeed: {e:#}"));
    let _ = std::fs::remove_dir_all(&dir);
    r.1
}

fn codes(d: &[CompilerDiagnostic]) -> Vec<&'static str> {
    d.iter().map(|d| d.code).collect()
}

fn errors(d: &[CompilerDiagnostic]) -> usize {
    d.iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .count()
}

const LEGACY: &str =
    "fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\nfn unused() { return 0 }\n";

const LEGACY_USER: &str = "import geo\nimport geo { helper }\nfn main() {\n    print(geo.area(2, 3))\n    print(geo.area(1, 1))\n    print(helper(1))\n}\n";

#[test]
fn gate_turns_a_legacy_module_used_across_packages_into_one_error_naming_every_use() {
    let d = compile(
        "legacy-gate",
        &[("lib/geo.crush", LEGACY), ("main.crush", LEGACY_USER)],
        true,
    );
    assert_eq!(codes(&d), [E_VIS01], "one per module, no note: {d:?}");
    let e = &d[0];
    assert_eq!(e.severity, DiagnosticSeverity::Error);
    assert!(
        e.message.contains("geo.crush"),
        "names the file: {}",
        e.message
    );
    assert!(
        e.message.contains("uses it: area, helper "),
        "every name used, once, sorted; not the unused one: {}",
        e.message
    );
    assert!(!e.message.contains("unused"), "{}", e.message);
    assert!(
        e.message.contains("the root package"),
        "names the user: {}",
        e.message
    );
    assert!(e.location.file.ends_with("main.crush"), "{:?}", e.location);
    assert_eq!(
        e.location.line, 2,
        "at the first use (the selective import)"
    );
}

#[test]
fn without_the_gate_the_same_program_gets_v1s_note() {
    let d = compile(
        "legacy-nogate",
        &[("lib/geo.crush", LEGACY), ("main.crush", LEGACY_USER)],
        false,
    );
    assert_eq!(codes(&d), [N_VIS01], "{d:?}");
    assert_eq!(d[0].severity, DiagnosticSeverity::Note);
    assert_eq!(errors(&d), 0);
}

const GEO: &str = "pub fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\n";

#[test]
fn gate_makes_w_vis01_an_error_and_off_it_stays_a_warning() {
    let files = [
        ("lib/geo.crush", GEO),
        (
            "main.crush",
            "import geo\nfn main() {\n    print(geo.area(2, 3))\n    print(geo.helper(1))\n}\n",
        ),
    ];
    let on = compile("wvis-gate", &files, true);
    assert_eq!(codes(&on), [W_VIS01], "{on:?}");
    assert_eq!(on[0].severity, DiagnosticSeverity::Error);
    let off = compile("wvis-nogate", &files, false);
    assert_eq!(codes(&off), [W_VIS01], "{off:?}");
    assert_eq!(off[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(
        on[0].message.replace("wvis-gate", "wvis-nogate"),
        off[0].message,
        "same text, only the level"
    );
}

#[test]
fn gate_passes_a_fully_pub_library_used_through_its_pub_names() {
    let d = compile(
        "pub-ok",
        &[
            ("lib/geo.crush", GEO),
            (
                "main.crush",
                "import geo\nimport geo { area }\nfn main() {\n    print(geo.area(2, 3))\n    print(area(1, 1))\n}\n",
            ),
        ],
        true,
    );
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn gate_passes_a_single_package_program_with_no_pub() {
    // Legacy modules inside one package are fine: stage 3 keeps them usable
    // there (no `pub` means package-private).
    let d = compile(
        "one-pkg",
        &[
            ("geo.crush", LEGACY),
            ("util/strings.crush", "fn twice(s) { return s + s }\n"),
            (
                "main.crush",
                "import geo\nimport util.strings\nfn main() {\n    print(geo.helper(1))\n    print(strings.twice(\"a\"))\n}\n",
            ),
        ],
        true,
    );
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn surface_of_a_pub_module_lists_pub_and_private_names() {
    let s = module_surface(
        "pub fn area(w, h) { return w * h }\nfn helper(x) { return x }\npub struct P { x: Int }\nstruct Q { y: Int }\n",
    )
    .unwrap();
    assert!(s.has_pub() && !s.is_legacy() && !s.fails_gate());
    assert_eq!(s.exported(), ["area", "P"]);
    assert_eq!(s.private(), ["helper", "Q"]);
    assert_eq!(
        render_surface("geo.crush", &s),
        "geo.crush: pub\n  pub: area, P\n  private: helper, Q\n"
    );
}

#[test]
fn surface_of_a_legacy_module_says_everything_public_and_fails_the_gate() {
    let s = module_surface(LEGACY).unwrap();
    assert!(s.is_legacy() && s.fails_gate());
    assert!(s.private().is_empty());
    assert_eq!(
        render_surface("geo.crush", &s),
        "geo.crush: legacy: no pub, everything public\n  public: area, helper, unused\n"
    );
}

#[test]
fn a_library_with_a_demo_main_is_importable_and_fails_the_gate() {
    // The resolver drops a module's `main`, so `helper` is importable.
    let s = module_surface("fn helper() { return 1 }\nfn main() { print(helper()) }\n").unwrap();
    assert!(
        s.has_main && s.is_importable() && s.is_legacy() && s.fails_gate(),
        "{s:?}"
    );
    assert_eq!(
        render_surface("lib.crush", &s),
        "lib.crush: legacy: no pub, everything public (its `fn main` is ignored when it is \
         imported)\n  public: helper\n"
    );
}

#[test]
fn a_file_with_top_level_statements_cannot_be_imported_and_is_exempt() {
    let s = module_surface("fn helper() { return 1 }\nprint(helper())\n").unwrap();
    assert!(
        s.has_toplevel_code && !s.is_importable() && !s.fails_gate(),
        "{s:?}"
    );
    assert_eq!(
        render_surface("app.crush", &s),
        "app.crush: program (exempt from the gate: top-level statements, so it cannot be \
         imported)\n"
    );
}

#[test]
fn a_use_lang_at_the_top_level_makes_the_file_unimportable_as_the_resolver_says() {
    // The resolver refuses a module with a `use @lang` (only Crush module
    // imports are allowed), so the gate must not call it importable.
    let src = "use @lang python \"math\"\nfn helper() { return 1 }\n";
    let s = module_surface(src).unwrap();
    assert!(
        s.has_toplevel_code && !s.is_importable() && !s.fails_gate(),
        "{s:?}"
    );
    // And the resolver does refuse it.
    let dir = tree(
        "use-lang",
        &[
            ("lib/m.crush", src),
            (
                "main.crush",
                "import m\nfn main() {\n    print(m.helper())\n}\n",
            ),
        ],
    );
    let err = crush_frontend::parse_file_reporting(
        &dir.join("main.crush"),
        &ImportOptions::new([dir.join("lib")]),
    )
    .unwrap_err();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(format!("{err:#}").contains("use @"), "{err:#}");
    // A Crush module import is not top-level code.
    let s = module_surface("import other\nfn helper() { return 1 }\n").unwrap();
    assert!(s.is_importable() && s.fails_gate(), "{s:?}");
}

#[test]
fn a_main_only_file_exports_nothing_and_is_exempt() {
    let s = module_surface("fn main() { print(1) }\n").unwrap();
    assert!(!s.is_legacy() && !s.fails_gate(), "{s:?}");
    assert_eq!(
        render_surface("m.crush", &s),
        "m.crush: program (exempt from the gate: nothing to export besides `main`)\n"
    );
}

#[test]
fn a_legacy_module_with_only_a_struct_fails_the_directory_gate() {
    // The compile gate sees function uses only; the directory gate sees kinds.
    let s = module_surface("struct P { x: Int }\nimpl P { fn norm(self) { return self.x } }\n")
        .unwrap();
    assert!(s.is_legacy() && s.fails_gate(), "{s:?}");
}

#[test]
fn a_pub_trait_alone_ends_the_legacy_rule_in_the_surface_too() {
    let s =
        module_surface("pub trait Shape { fn area(self) }\nfn helper() { return 1 }\n").unwrap();
    assert!(s.has_pub() && !s.fails_gate(), "{s:?}");
    assert_eq!(s.private(), ["helper"]);
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap().display().to_string()
}

#[test]
fn surfaces_walks_a_directory_sorted_and_skips_hidden_and_target() {
    let dir = tree(
        "walk",
        &[
            ("b.crush", GEO),
            ("a/z.crush", LEGACY),
            (".git/x.crush", LEGACY),
            ("target/y.crush", LEGACY),
            ("notes.txt", "not crush"),
            ("bad.crush", "fn ( {"),
        ],
    );
    let found = surfaces(&dir).unwrap();
    let names: Vec<String> = found.iter().map(|(p, _)| rel(&dir, p)).collect();
    assert_eq!(names, ["a/z.crush", "b.crush", "bad.crush"]);
    assert!(found[0].1.as_ref().unwrap().fails_gate());
    assert!(!found[1].1.as_ref().unwrap().fails_gate());
    assert!(found[2].1.is_err(), "a parse error comes back as an error");
    let one = surfaces(&dir.join("b.crush")).unwrap();
    assert_eq!(one.len(), 1);
    let refused = surfaces(&dir.join("notes.txt")).unwrap_err();
    assert_eq!(
        refused.kind(),
        std::io::ErrorKind::InvalidInput,
        "{refused}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn surfaces_does_not_follow_a_symlinked_directory() {
    let dir = tree("symlink", &[("a/z.crush", LEGACY)]);
    // A loop back to the root, and a second route to `a/`.
    std::os::unix::fs::symlink(&dir, dir.join("a/loop")).unwrap();
    std::os::unix::fs::symlink(dir.join("a"), dir.join("again")).unwrap();
    let found = surfaces(&dir).unwrap();
    let names: Vec<String> = found.iter().map(|(p, _)| rel(&dir, p)).collect();
    assert_eq!(names, ["a/z.crush"], "listed once, no loop");
    let empty = tree("empty", &[("notes.txt", "x")]);
    assert!(surfaces(&empty).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&empty);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gate_catches_a_legacy_module_reached_through_a_pub_one_in_another_package() {
    // main (root) -> pub b (lib2) -> legacy c (lib3).
    let dir = tree(
        "transitive",
        &[
            ("lib3/c.crush", "fn deep() { return 7 }\n"),
            (
                "lib2/b.crush",
                "import c\npub fn mid() { return c.deep() }\n",
            ),
            (
                "main.crush",
                "import b\nfn main() {\n    print(b.mid())\n}\n",
            ),
        ],
    );
    let mut opts = ImportOptions::new([dir.join("lib2"), dir.join("lib3")]);
    opts.deny_legacy_visibility = true;
    let (_, d) = crush_frontend::parse_file_reporting(&dir.join("main.crush"), &opts).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(codes(&d), [E_VIS01], "{d:?}");
    assert!(d[0].message.contains("c.crush"), "{}", d[0].message);
    assert!(d[0].message.contains("uses it: deep "), "{}", d[0].message);
    assert!(
        d[0].location.file.ends_with("b.crush"),
        "{:?}",
        d[0].location
    );
}
