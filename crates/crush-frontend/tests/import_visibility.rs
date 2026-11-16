//! Module visibility, stage 1 (V1, #519; `docs/design/visibility-and-packages.md`
//! §1, ruling 1 "C"): `pub` is recorded, every file belongs to a package
//! (longest containing root wins), and a cross-package use of a name that is
//! not `pub` is the warning `W-VIS01` on all three access paths. A module with
//! no `pub` anywhere is all-public and gets one `N-VIS01` note. Findings never
//! stop the compile.

use std::path::{Path, PathBuf};

use crush_cast::{Program, Statement};
use crush_frontend::diagnostics::{CompilerDiagnostic, DiagnosticSeverity};
use crush_frontend::import_system::{ImportOptions, ImportPackage, N_VIS01, W_VIS01};

/// A fresh directory under the system temp dir holding `files`.
fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "crush-import-visibility-{tag}-{}",
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

/// Compile `<dir>/<entry>` with `-L` dirs `libs` (relative to the tree);
/// the program and its visibility findings.
fn compile_with(
    tag: &str,
    files: &[(&str, &str)],
    entry: &str,
    opts: impl Fn(&Path) -> ImportOptions,
) -> (Program, Vec<CompilerDiagnostic>) {
    let dir = tree(tag, files);
    let r = crush_frontend::parse_file_reporting(&dir.join(entry), &opts(&dir))
        .unwrap_or_else(|e| panic!("{tag}: the compile must succeed: {e:#}"));
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn compile(tag: &str, files: &[(&str, &str)], libs: &[&str]) -> Vec<CompilerDiagnostic> {
    compile_with(tag, files, "main.crush", |dir| {
        ImportOptions::new(libs.iter().map(|l| dir.join(l)))
    })
    .1
}

fn codes(d: &[CompilerDiagnostic]) -> Vec<&'static str> {
    d.iter().map(|d| d.code).collect()
}

const GEO: &str =
    "pub fn area(w, h) { return w * h }\nfn helper(x) { return x + 1 }\nfn other() { return 0 }\n";

#[test]
fn same_package_use_of_a_private_name_is_silent() {
    let d = compile(
        "same-pkg",
        &[
            ("geo.crush", GEO),
            (
                "sub/more.crush",
                "pub fn m() { return 1 }\nfn hidden() { return 2 }\n",
            ),
            (
                "main.crush",
                "import geo\nimport geo { other }\nimport sub.more\nfn main() {\n    print(geo.helper(1))\n    let f = geo.other\n    print(more.hidden())\n}\n",
            ),
        ],
        &[],
    );
    assert!(d.is_empty(), "one package, so no finding: {d:?}");
}

#[test]
fn cross_package_qualified_call_of_a_private_name_warns() {
    let d = compile(
        "call",
        &[
            ("lib/geo.crush", GEO),
            (
                "main.crush",
                "import geo\nfn main() {\n    print(geo.area(2, 3))\n    print(geo.helper(1))\n}\n",
            ),
        ],
        &["lib"],
    );
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    let w = &d[0];
    assert_eq!(w.severity, DiagnosticSeverity::Warning);
    assert!(w.message.contains("`helper` is not `pub`"), "{}", w.message);
    assert!(
        w.message.contains("geo.crush"),
        "names the module: {}",
        w.message
    );
    assert!(
        w.message.contains("the library directory") && w.message.contains("lib"),
        "names the package: {}",
        w.message
    );
    assert!(
        w.message.contains("`geo.helper(…)`"),
        "names the path: {}",
        w.message
    );
    assert!(
        w.hint
            .as_deref()
            .unwrap()
            .contains("add `pub` to `fn helper`"),
        "names the fix: {:?}",
        w.hint
    );
    assert!(w.location.file.ends_with("main.crush"), "{:?}", w.location);
    assert_eq!(w.location.line, 4);
}

#[test]
fn cross_package_function_value_of_a_private_name_warns() {
    let d = compile(
        "value",
        &[
            ("lib/geo.crush", GEO),
            (
                "main.crush",
                "import geo\nfn main() {\n    let f = geo.other\n    print(f())\n}\n",
            ),
        ],
        &["lib"],
    );
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    assert!(
        d[0].message.contains("`other` is not `pub`"),
        "{}",
        d[0].message
    );
    assert!(
        d[0].message.contains("function value `geo.other`"),
        "{}",
        d[0].message
    );
    assert_eq!(d[0].location.line, 3);
}

#[test]
fn cross_package_selective_import_of_a_private_name_warns_at_the_import() {
    let d = compile(
        "selective",
        &[
            ("lib/geo.crush", GEO),
            (
                "main.crush",
                "import geo { helper }\nfn main() {\n    print(helper(1))\n    print(helper(2))\n}\n",
            ),
        ],
        &["lib"],
    );
    // One finding, at the import: the bare calls it binds are not re-reported.
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    assert!(
        d[0].message.contains("`import geo { helper }`"),
        "{}",
        d[0].message
    );
    assert_eq!(d[0].location.line, 1);
}

#[test]
fn cross_package_use_of_a_pub_name_is_silent() {
    let d = compile(
        "pub-use",
        &[
            ("lib/geo.crush", GEO),
            (
                "main.crush",
                "import geo\nimport geo { area }\nfn main() {\n    print(geo.area(2, 3))\n    let f = geo.area\n    print(area(1, 1))\n}\n",
            ),
        ],
        &["lib"],
    );
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn the_first_hidden_use_lists_what_the_modules_pub_hid_once() {
    let d = compile(
        "listing",
        &[
            ("lib/geo.crush", GEO),
            (
                "main.crush",
                "import geo\nfn main() {\n    print(geo.helper(1))\n    print(geo.other())\n}\n",
            ),
        ],
        &["lib"],
    );
    assert_eq!(codes(&d), [W_VIS01, W_VIS01], "{d:?}");
    let first = d[0].hint.as_deref().unwrap();
    assert!(
        first.contains("names it does not mark are private") && first.contains("helper, other"),
        "the first finding lists the hidden names: {first}"
    );
    assert!(first.contains("(`pub`: area)"), "{first}");
    let second = d[1].hint.as_deref().unwrap();
    assert!(
        !second.contains("does not mark"),
        "listed once per module: {second}"
    );
}

#[test]
fn a_legacy_module_gets_one_note_and_no_warning() {
    let d = compile(
        "legacy",
        &[
            (
                "old/legacy.crush",
                "fn twice(x) { return x * 2 }\nfn thrice(x) { return x * 3 }\nfn __inner() { return 0 }\n",
            ),
            (
                "main.crush",
                "import legacy\nimport legacy { thrice }\nfn main() {\n    print(legacy.twice(1))\n    print(legacy.thrice(1))\n    let f = legacy.__inner\n}\n",
            ),
        ],
        &["old"],
    );
    assert_eq!(codes(&d), [N_VIS01], "{d:?}");
    assert_eq!(d[0].severity, DiagnosticSeverity::Note);
    assert!(d[0].message.contains("has no `pub`"), "{}", d[0].message);
    assert!(d[0].message.contains("legacy.crush"), "{}", d[0].message);
}

#[test]
fn a_pub_struct_alone_ends_the_legacy_rule_for_its_module() {
    // "No `pub` anywhere" counts kinds too: one `pub struct` makes the
    // module's unmarked functions private to its package.
    let d = compile(
        "pub-struct-only",
        &[
            (
                "lib/shapes.crush",
                "pub struct Sq { side }\nfn make(s) { return s }\n",
            ),
            (
                "main.crush",
                "import shapes\nfn main() {\n    print(shapes.make(1))\n}\n",
            ),
        ],
        &["lib"],
    );
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
}

#[test]
fn the_longest_containing_root_decides_the_package() {
    // `lib/inner/deep.crush` sits under two `-L` roots; `lib/inner` is
    // longer, so `deep` is its own package, and `lib/top.crush` (package
    // `lib`) reaching its private `hidden` warns.
    let files = [
        (
            "lib/inner/deep.crush",
            "pub fn shown() { return 1 }\nfn hidden() { return 2 }\n",
        ),
        (
            "lib/top.crush",
            "import inner.deep\npub fn go() { return deep.hidden() }\n",
        ),
        (
            "main.crush",
            "import top\nfn main() {\n    print(top.go())\n}\n",
        ),
    ];
    let (program, d) = compile_with("nested", &files, "main.crush", |dir| {
        ImportOptions::new([dir.join("lib"), dir.join("lib/inner")])
    });
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    assert!(
        d[0].location.file.ends_with("top.crush"),
        "{:?}",
        d[0].location
    );
    let pkg = |f: &str| {
        program.functions[f]
            .meta
            .get("package")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        pkg("inner::deep::hidden").ends_with("inner"),
        "{}",
        pkg("inner::deep::hidden")
    );
    assert!(pkg("top::go").ends_with("lib"), "{}", pkg("top::go"));

    // With only the outer root, the two files share a package: silent.
    let (_, d) = compile_with("nested-outer", &files, "main.crush", |dir| {
        ImportOptions::new([dir.join("lib")])
    });
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn a_file_reached_by_two_routes_has_one_package() {
    // `lib/shared.crush` is reached relatively (`import lib.shared`, from the
    // entry's directory) and through `-L lib` (`import shared`), and also
    // from a second module (a diamond). The file decides, not the route: it
    // is in the `lib` package every time, so every private use warns alike.
    let files = [
        (
            "lib/shared.crush",
            "pub fn ok() { return 1 }\nfn priv_fn() { return 2 }\n",
        ),
        (
            "lib/a.crush",
            "import shared\npub fn fa() { return shared.priv_fn() }\n",
        ),
        (
            "main.crush",
            "import lib.shared\nimport shared as s2\nimport a\nfn main() {\n    print(shared.priv_fn())\n    print(s2.priv_fn())\n    print(a.fa())\n}\n",
        ),
    ];
    let (program, d) = compile_with("diamond", &files, "main.crush", |dir| {
        ImportOptions::new([dir.join("lib")])
    });
    // a.crush is in `lib` with shared.crush: silent. The two uses in
    // main.crush (root package) both warn.
    assert_eq!(codes(&d), [W_VIS01, W_VIS01], "{d:?}");
    assert!(
        d.iter().all(|w| w.location.file.ends_with("main.crush")),
        "{d:?}"
    );
    let shared: Vec<_> = program
        .functions
        .iter()
        .filter(|(n, _)| n.ends_with("::priv_fn"))
        .collect();
    assert_eq!(shared.len(), 1, "loaded once");
    let pkg = shared[0].1.meta["package"].as_str().unwrap();
    assert!(pkg.starts_with("dir:") && pkg.ends_with("lib"), "{pkg}");
}

#[test]
fn an_input_without_a_real_file_is_in_the_root_package() {
    // The REPL resolves from `<cwd>/<repl>`, a file that does not exist: it
    // is in the root package, the directory it names, so a module beside
    // it shares its package.
    let dir = tree("pathless", &[("geo.crush", GEO)]);
    let mut program =
        crush_frontend::parse_source("import geo\nfn main() {\n    print(geo.helper(1))\n}\n")
            .unwrap();
    let d = crush_frontend::import_system::resolve_imports_reporting(
        &mut program,
        &dir.join("<repl>"),
        &ImportOptions::default(),
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(d.is_empty(), "{d:?}");
    // And a string entry point resolves nothing, so it has nothing to report.
    let program =
        crush_frontend::parse_source("pub fn f() { return 1 }\nfn main() { print(f()) }\n")
            .unwrap();
    assert!(crush_frontend::check_program(&program).is_empty());
}

#[test]
fn the_home_package_root_is_the_root_package() {
    // crush-pkg compiles `tests/t.crush` with its own `src/` as a named
    // package; `home_package` names it, so tests reach private helpers
    // without a warning, as Go's in-package tests do.
    let files = [
        (
            "src/main.crush",
            "pub fn api() { return 1 }\nfn helper() { return 2 }\n",
        ),
        (
            "tests/t.crush",
            "import app\nfn main() {\n    print(app.helper())\n}\n",
        ),
    ];
    let opts = |dir: &Path, home: Option<&str>| {
        let mut o = ImportOptions::new([dir.join("src")]);
        o.packages.push(ImportPackage {
            name: "app".into(),
            root: dir.join("src"),
            entry: dir.join("src/main.crush"),
        });
        o.home_package = home.map(str::to_string);
        o
    };
    let (_, d) = compile_with("home", &files, "tests/t.crush", |dir| {
        opts(dir, Some("app"))
    });
    assert!(d.is_empty(), "{d:?}");
    let (_, d) = compile_with("not-home", &files, "tests/t.crush", |dir| opts(dir, None));
    assert_eq!(codes(&d), [W_VIS01], "{d:?}");
    assert!(d[0].message.contains("package `app`"), "{}", d[0].message);
}

fn def_meta<'a>(
    program: &'a Program,
    kind: &str,
    name: &str,
) -> &'a std::collections::HashMap<String, serde_json::Value> {
    program.functions["main"]
        .body
        .iter()
        .find_map(|s| match s {
            Statement::StructDef { name: n, meta, .. } if kind == "struct" && n == name => {
                Some(meta)
            }
            Statement::TraitDef { name: n, meta, .. } if kind == "trait" && n == name => Some(meta),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {kind} {name}"))
}

fn is_pub(meta: &std::collections::HashMap<String, serde_json::Value>) -> bool {
    meta.get("pub").and_then(|v| v.as_bool()).unwrap_or(false)
}

#[test]
fn pub_is_recorded_on_fn_async_fn_struct_and_trait() {
    let program = crush_frontend::parse_source(
        "pub struct P { x: Int }\nstruct Q { y }\n@kind(semantics: value(copy))\npub struct V { v }\npub trait T {\n    fn f(self);\n}\ntrait U {\n    fn g(self);\n}\npub fn a() { return 1 }\npub async fn b() { return 2 }\nfn c() { return 3 }\nfn main() { print(a()) }\n",
    )
    .unwrap_or_else(|e| panic!("{e:#}"));
    assert!(is_pub(def_meta(&program, "struct", "P")));
    assert!(!is_pub(def_meta(&program, "struct", "Q")));
    assert!(is_pub(def_meta(&program, "struct", "V")));
    assert!(is_pub(def_meta(&program, "trait", "T")));
    assert!(!is_pub(def_meta(&program, "trait", "U")));
    assert!(is_pub(&program.functions["a"].meta));
    assert!(is_pub(&program.functions["b"].meta));
    assert!(program.functions["b"].is_async);
    assert!(!is_pub(&program.functions["c"].meta));
}

#[test]
fn an_identifier_named_pub_still_works() {
    // `pub` is contextual: a variable, a field and a function may be named
    // `pub`, and it is a modifier only before `fn` / `async fn` / `struct` /
    // `trait`.
    let program = crush_frontend::parse_source(
        "struct R { pub: Int }\nfn pub() { return 4 }\nfn main() {\n    let pub = 1\n    pub = pub + 1\n    print(pub)\n    let r = R { pub: 2 }\n    print(r.pub)\n}\n",
    )
    .unwrap_or_else(|e| panic!("{e:#}"));
    assert!(program.functions.contains_key("pub"));
    let fields = program.functions["main"].body.iter().find_map(|s| match s {
        Statement::StructDef { name, fields, .. } if name == "R" => Some(fields.clone()),
        _ => None,
    });
    assert_eq!(fields.unwrap()[0].0, "pub");
    crush_frontend::compile_cast(&program).unwrap_or_else(|e| panic!("{e:#}"));
}

#[test]
fn pub_fn_with_a_dunder_name_is_linted() {
    let files = [
        (
            "lib/m.crush",
            "pub fn __leak() { return 1 }\npub fn fine() { return 2 }\nfn __quiet() { return 3 }\n",
        ),
        (
            "main.crush",
            "import m\nfn main() {\n    print(m.fine())\n}\n",
        ),
    ];
    let (program, d) = compile_with("dunder", &files, "main.crush", |dir| {
        ImportOptions::new([dir.join("lib")])
    });
    assert!(d.is_empty(), "{d:?}");
    let lint = crush_frontend::check_program(&program);
    let w: Vec<_> = lint.iter().filter(|d| d.code == "W-VIS02").collect();
    assert_eq!(w.len(), 1, "{lint:?}");
    assert!(w[0].message.contains("`pub fn __leak`"), "{}", w[0].message);
    assert!(
        w[0].location.file.ends_with("m.crush"),
        "{:?}",
        w[0].location
    );
    assert_eq!(w[0].location.line, 1);
}

#[test]
fn findings_do_not_change_the_compiled_program() {
    // A warning, not an error: the same program compiles with or without it.
    let files = [
        ("lib/geo.crush", GEO),
        (
            "main.crush",
            "import geo\nfn main() {\n    print(geo.helper(1))\n}\n",
        ),
    ];
    let (program, d) = compile_with("compiles", &files, "main.crush", |dir| {
        ImportOptions::new([dir.join("lib")])
    });
    assert_eq!(codes(&d), [W_VIS01]);
    crush_frontend::compile_cast(&program).unwrap_or_else(|e| panic!("{e:#}"));
}
