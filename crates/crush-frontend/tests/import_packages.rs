//! Named package roots (`ImportOptions::packages`): how crush-pkg makes a
//! dependency importable by its name, without concatenating sources.

use std::path::{Path, PathBuf};

use crush_frontend::import_system::{ImportOptions, ImportPackage};

/// A fresh directory under the system temp dir holding `files`.
fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("crush-import-pkg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

fn pkg(name: &str, root: &Path, entry: &str) -> ImportPackage {
    ImportPackage { name: name.into(), root: root.to_path_buf(), entry: root.join(entry) }
}

fn opts(packages: Vec<ImportPackage>) -> ImportOptions {
    ImportOptions { packages, ..Default::default() }
}

#[test]
fn package_name_loads_its_entry_file_whatever_it_is_called() {
    let dir = tree(
        "entry",
        &[
            ("lib/src/lib.crush", "fn greet(n) {\n    return \"hi \" + n\n}\n"),
            ("app/main.crush", "import mylib { greet }\nfn main() {\n    io.print(greet(\"x\"))\n}\n"),
        ],
    );
    let program = crush_frontend::parse_file(
        &dir.join("app/main.crush"),
        &opts(vec![pkg("mylib", &dir.join("lib"), "src/lib.crush")]),
    )
    .expect("import through a package name");
    assert!(program.functions.contains_key("mylib::greet"), "{:?}", program.functions.keys());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dotted_import_resolves_inside_the_package_root() {
    let dir = tree(
        "dotted",
        &[
            ("lib/lib.crush", "fn a() {\n    return 1\n}\n"),
            ("lib/text/case.crush", "fn up(s) {\n    return s\n}\n"),
            ("app/main.crush", "import mylib.text.case as c\nfn main() {\n    io.print(c.up(\"x\"))\n}\n"),
        ],
    );
    let program = crush_frontend::parse_file(
        &dir.join("app/main.crush"),
        &opts(vec![pkg("mylib", &dir.join("lib"), "lib.crush")]),
    )
    .expect("dotted package import");
    assert!(program.functions.contains_key("mylib::text::case::up"), "{:?}", program.functions.keys());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_package_wins_over_a_same_named_file_next_to_the_importer() {
    let dir = tree(
        "shadow",
        &[
            ("lib/real.crush", "fn which() {\n    return \"package\"\n}\n"),
            ("app/mylib.crush", "fn which() {\n    return \"local file\"\n}\n"),
            ("app/main.crush", "import mylib\nfn main() {\n    io.print(mylib.which())\n}\n"),
        ],
    );
    let program = crush_frontend::parse_file(
        &dir.join("app/main.crush"),
        &opts(vec![pkg("mylib", &dir.join("lib"), "real.crush")]),
    )
    .unwrap();
    let f = &program.functions["mylib::which"];
    let file = f.meta.get("source_file").and_then(|v| v.as_str()).unwrap();
    assert!(file.ends_with("lib/real.crush"), "{file}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_inside_a_package_names_the_package() {
    let dir = tree(
        "missing",
        &[
            ("lib/lib.crush", "fn a() {\n    return 1\n}\n"),
            ("app/main.crush", "import mylib.nope\nfn main() {\n}\n"),
        ],
    );
    let err = crush_frontend::parse_file(
        &dir.join("app/main.crush"),
        &opts(vec![pkg("mylib", &dir.join("lib"), "lib.crush")]),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("package `mylib`") && err.contains("nope.crush"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn not_found_lists_the_importable_package_names() {
    let dir = tree(
        "notfound",
        &[("lib/lib.crush", "fn a() {\n    return 1\n}\n"), ("app/main.crush", "import other\nfn main() {\n}\n")],
    );
    let err = crush_frontend::parse_file(
        &dir.join("app/main.crush"),
        &opts(vec![pkg("mylib", &dir.join("lib"), "lib.crush")]),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("module not found") && err.contains("packages importable by name: mylib"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
