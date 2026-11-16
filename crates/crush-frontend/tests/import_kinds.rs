//! Kind names across imports (#515): `struct` / `trait` names are one global
//! namespace, so the same name declared in two different files is a compile
//! error naming both. It used to compile, and the later declaration silently
//! replaced the earlier one, so the first module's code failed at run time.

use std::path::PathBuf;

use crush_frontend::import_system::ImportOptions;

/// A fresh directory under the system temp dir holding `files`.
fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("crush-import-kinds-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

fn compile(tag: &str, files: &[(&str, &str)]) -> Result<(), String> {
    let dir = tree(tag, files);
    let r = crush_frontend::parse_file(&dir.join("main.crush"), &ImportOptions::default())
        .map(|_| ())
        .map_err(|e| format!("{e:#}"));
    let _ = std::fs::remove_dir_all(&dir);
    r
}

const A: &str = "struct P { x: Int }\nfn make_a() {\n    return P { x: 1 }\n}\n";
const B: &str = "struct P { y: Int }\nfn make_b() {\n    return P { y: 2 }\n}\n";

#[test]
fn two_modules_declaring_the_same_struct_is_a_compile_error_naming_both() {
    let err = compile(
        "two-modules",
        &[
            ("a.crush", A),
            ("b.crush", B),
            (
                "main.crush",
                "import a\nimport b\nfn main() {\n    io.print(a.make_a())\n}\n",
            ),
        ],
    )
    .expect_err("a kind-name clash between two modules must not compile");
    assert!(err.contains("struct `P` is also declared in"), "{err}");
    assert!(err.contains("a.crush:1:"), "first declaration named: {err}");
    assert!(err.contains("b.crush"), "second file named: {err}");
}

#[test]
fn importer_and_module_declaring_the_same_struct_is_a_compile_error() {
    let err = compile(
        "importer",
        &[
            ("a.crush", A),
            (
                "main.crush",
                "import a\nstruct P { z: Int }\nfn main() {\n    io.print(a.make_a())\n}\n",
            ),
        ],
    )
    .expect_err("the importer's own struct clashing with a module's must not compile");
    assert!(err.contains("struct `P` is also declared in"), "{err}");
    assert!(err.contains("main.crush"), "{err}");
}

#[test]
fn a_struct_and_a_trait_of_the_same_name_in_two_files_clash() {
    let err = compile(
        "struct-trait",
        &[
            ("a.crush", A),
            ("t.crush", "trait P {\n    fn f(self);\n}\n"),
            (
                "main.crush",
                "import a\nimport t\nfn main() {\n    io.print(a.make_a())\n}\n",
            ),
        ],
    )
    .expect_err("a trait named like another file's struct must not compile");
    assert!(err.contains("trait `P` is also declared in"), "{err}");
}

#[test]
fn a_diamond_shares_one_declaration_and_compiles() {
    // main imports l and r; both import shapes. shapes.crush is loaded once,
    // so `struct P` is declared once: no clash.
    compile(
        "diamond",
        &[
            ("shapes.crush", A),
            ("l.crush", "import shapes\nfn left() {\n    return shapes.make_a()\n}\n"),
            ("r.crush", "import shapes\nfn right() {\n    return shapes.make_a()\n}\n"),
            (
                "main.crush",
                "import l\nimport r\nfn main() {\n    io.print(l.left())\n    io.print(r.right())\n}\n",
            ),
        ],
    )
    .expect("a diamond import declares each Kind once");
}

#[test]
fn distinct_kind_names_across_modules_compile() {
    compile(
        "distinct",
        &[
            ("a.crush", A),
            ("q.crush", "struct Q { y: Int }\nfn make_q() {\n    return Q { y: 2 }\n}\n"),
            (
                "main.crush",
                "import a\nimport q\nfn main() {\n    io.print(a.make_a())\n    io.print(q.make_q())\n}\n",
            ),
        ],
    )
    .expect("different Kind names in different modules are fine");
}

#[test]
fn a_redeclaration_inside_one_file_is_unchanged_by_this_check() {
    // Same-file `struct P` twice (no `@kind`, no methods) keeps its old
    // behaviour: the later declaration wins. Only cross-file clashes are #515.
    compile(
        "same-file",
        &[
            ("a.crush", "fn helper() {\n    return 1\n}\n"),
            (
                "main.crush",
                "import a\nstruct P { x: Int }\nstruct P { y: Int }\nfn main() {\n    io.print(P { y: a.helper() })\n}\n",
            ),
        ],
    )
    .expect("a same-file redeclaration is not an import error");
}
