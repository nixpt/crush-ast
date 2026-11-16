//! Owner ids in the compiled artifact (V3b, #522): a Kind's `module` and
//! `package`, and the owning package of each function (COSM `fn_packages`).
//!
//! `module` comes from the import system today. The package comes from
//! `meta["package"]`, which the V1 resolver (#519) records; until then these
//! tests set it by hand, the way that resolver will.

use std::collections::HashMap;
use std::path::PathBuf;

use crush_cast::Statement;
use crush_frontend::import_system::ImportOptions;
use serde_json::{Value, json};

fn tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("crush-owner-ids-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, src).unwrap();
    }
    dir
}

fn set_package(meta: &mut HashMap<String, Value>, id: &str) {
    meta.insert("package".to_string(), json!(id));
}

const GEO: &str = "struct P { x: Int }\nimpl P {\n    fn get(self) {\n        return self.x\n    }\n}\nfn make() {\n    return P { x: 1 }\n}\n";
const GEO_MAIN: &str =
    "import geo\nstruct Q { y: Int }\nfn main() {\n    io.print(geo.make().get())\n}\n";

fn parse_tree(tag: &str, files: &[(&str, &str)]) -> crush_cast::Program {
    let dir = tree(tag, files);
    let program =
        crush_frontend::parse_file(&dir.join("main.crush"), &ImportOptions::default()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    program
}

/// Ruling on independent review (altair, 2026-11-16): a Kind of the root
/// package records no owner, wherever it is declared, so a one-package
/// multi-file program is written as before and old runtimes still run it.
#[test]
fn a_kind_from_an_imported_module_of_the_root_package_records_no_owner() {
    let program = parse_tree("module", &[("geo.crush", GEO), ("main.crush", GEO_MAIN)]);
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.expect("kinds");
    assert_eq!(casm.cosm.as_ref(), Some(&cosm));
    for k in &cosm.kinds {
        assert_eq!(
            (k.module.as_deref(), k.package.as_deref()),
            (None, None),
            "{}",
            k.name
        );
    }
    assert!(!cosm.has_owners());
    assert_eq!(cosm.required_version(), crush_cosm::COSM_BASE_VERSION);
    assert!(!cosm.to_json().contains("cosm_version"));
}

/// A CASM program as a JSON document, after a round trip through `format`.
/// CASM keeps functions in a HashMap, so its bytes follow hash order even
/// between two compiles of one program: tests compare documents.
fn document(p: &casm::Program, format: casm::Format) -> Value {
    let back = casm::Program::deserialize(&p.serialize(format).unwrap(), format).unwrap();
    serde_json::from_slice(&back.serialize(casm::Format::Json).unwrap()).unwrap()
}

/// Strip what this PR's import system adds to merged declarations.
fn without_owner_stamps(mut program: crush_cast::Program) -> crush_cast::Program {
    for f in program.functions.values_mut() {
        for stmt in &mut f.body {
            if let Statement::StructDef { meta, .. }
            | Statement::ImplBlock { meta, .. }
            | Statement::TraitDef { meta, .. } = stmt
            {
                meta.remove("module");
                meta.remove(crush_frontend::cosm_lower::MODULE_PREFIX_META);
            }
        }
    }
    program
}

/// satya's L1: the declaration stamps must not reach compiled output of a
/// program without owners. A trait-only and a struct+impl imported module
/// compile to the same CASM (JSON and binary) with and without them.
#[test]
fn owner_stamps_leave_the_casm_of_a_one_package_program_unchanged() {
    let trait_mod = "trait Shape {\n    fn area(self);\n    fn describe(self) {\n        return \"shape\"\n    }\n}\n";
    let trait_main = "import t\nstruct P { x: Int }\nimpl Shape for P {\n    fn area(self) {\n        return self.x\n    }\n}\nfn main() {\n    let p = P { x: 2 }\n    io.print(p.area())\n    io.print(p.describe())\n}\n";
    for (tag, files) in [
        (
            "casm-trait",
            vec![("t.crush", trait_mod), ("main.crush", trait_main)],
        ),
        (
            "casm-struct",
            vec![("geo.crush", GEO), ("main.crush", GEO_MAIN)],
        ),
    ] {
        let stamped = parse_tree(tag, &files);
        let plain = without_owner_stamps(stamped.clone());
        let a = crush_frontend::compile_cast(&stamped).unwrap();
        let b = crush_frontend::compile_cast(&plain).unwrap();
        for format in [casm::Format::Json, casm::Format::Binary] {
            assert_eq!(
                document(&a, format),
                document(&b, format),
                "{tag} {format:?}"
            );
        }
        let text = String::from_utf8(a.serialize(casm::Format::Json).unwrap()).unwrap();
        assert!(!text.contains("module_prefix"), "{tag}");
        assert!(!text.contains("cosm_version"), "{tag}");
    }
}

/// The V1 seam: the resolver stamps the module's functions with a package
/// (here by hand, after the import system ran). The struct and impl of the
/// same module get it too (`inherit_declaration_owners`), so the Kind and
/// every lifted method are owned by `pkg:x`, and the Kind records its module.
#[test]
fn a_module_s_types_take_the_package_of_its_functions() {
    let mut program = parse_tree("seam", &[("geo.crush", GEO), ("main.crush", GEO_MAIN)]);
    for (name, f) in program.functions.iter_mut() {
        if name.starts_with("geo::") {
            set_package(&mut f.meta, "pkg:x");
        }
        // The declarations as a resolver that stamps only functions leaves
        // them: no package (derived from the functions above).
        for stmt in &mut f.body {
            if let Statement::StructDef { meta, .. }
            | Statement::ImplBlock { meta, .. }
            | Statement::TraitDef { meta, .. } = stmt
            {
                meta.remove("package");
            }
        }
    }
    let (_, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.unwrap();
    let p = cosm.kind("P").unwrap();
    assert_eq!(p.package.as_deref(), Some("pkg:x"));
    assert_eq!(p.module.as_deref(), Some("geo"));
    let q = cosm.kind("Q").unwrap();
    assert_eq!((q.module.as_deref(), q.package.as_deref()), (None, None));
    assert_eq!(cosm.function_package("geo::make"), Some("pkg:x"));
    assert_eq!(cosm.function_package("P::get"), Some("pkg:x"));
    assert_eq!(cosm.function_package("main"), None);
    assert_eq!(cosm.required_version(), crush_cosm::COSM_OWNERS_VERSION);
}

#[test]
fn a_module_split_across_packages_is_an_internal_error() {
    let two = "struct P { x: Int }\nfn make() {\n    return P { x: 1 }\n}\nfn other() {\n    return 2\n}\n";
    let mut program = parse_tree(
        "split",
        &[
            ("geo.crush", two),
            (
                "main.crush",
                "import geo\nfn main() {\n    io.print(geo.other())\n}\n",
            ),
        ],
    );
    set_package(
        &mut program.functions.get_mut("geo::make").unwrap().meta,
        "pkg:x",
    );
    set_package(
        &mut program.functions.get_mut("geo::other").unwrap().meta,
        "pkg:y",
    );
    let err = crush_frontend::compile_cast_with_cosm(program)
        .map(|_| ())
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("internal compiler error"),
        "{err:#}"
    );
}

#[test]
fn a_method_s_own_raw_package_is_normalized_before_it_is_recorded() {
    // CAST JSON may put a package on a method itself; it goes through the
    // same normalization as everything else.
    let src = "struct P { x: Int }\nimpl P {\n    fn get(self) {\n        return self.x\n    }\n}\nfn main() {\n    let p = P { x: 1 }\n    io.print(p.get())\n}\n";
    let mut program = crush_frontend::parse_source(src).unwrap();
    for f in program.functions.values_mut() {
        for stmt in &mut f.body {
            if let Statement::ImplBlock { methods, .. } = stmt {
                for m in methods {
                    set_package(&mut m.meta, "dir:/opt/someone/lib");
                }
            }
        }
    }
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    assert_eq!(cosm.unwrap().function_package("P::get"), Some("dir:lib"));
    let text = String::from_utf8(casm.serialize(casm::Format::Json).unwrap()).unwrap();
    assert!(!text.contains("/opt/someone"), "{text}");
}

#[test]
fn a_walker_module_with_an_invalid_owner_id_does_not_compile() {
    for bad in ["root", "lib", "pkg:a/b", "dir:"] {
        let mut program =
            crush_frontend::parse_source("fn main() {\n    io.print(1)\n}\n").unwrap();
        let mut k = crush_cosm::KindDecl::crush("W");
        k.package = Some(bad.to_string());
        program.cosm = Some(crush_cosm::CosmModule {
            kinds: vec![k],
            ..Default::default()
        });
        let err = crush_frontend::compile_cast_with_cosm(program)
            .map(|_| ())
            .unwrap_err();
        assert!(
            format!("{err:#}").contains("invalid_owner_id"),
            "{bad}: {err:#}"
        );
    }
}

#[test]
fn a_one_file_program_writes_no_owner() {
    let src = "struct P { x: Int }\nfn main() {\n    let p = P { x: 1 }\n    io.print(p.x)\n}\n";
    let (casm, cosm) = crush_frontend::compile_crush_source_with_cosm(src).unwrap();
    let cosm = cosm.unwrap();
    assert!(!cosm.has_owners());
    assert_eq!(cosm.required_version(), crush_cosm::COSM_BASE_VERSION);
    assert!(!cosm.to_json().contains("cosm_version"));
    assert_eq!(casm.cosm, Some(cosm));
}

const SRC: &str = "\
fn helper(n) {
    let h = |x| => x * 2
    return h(n)
}
fn other() {
    return 2
}
fn third() {
    return 3
}
fn main() {
    io.print(helper(1) + other() + third())
}
";

/// The V1 seam: package ids as the resolver records them, absolute
/// directories included, become stable artifact ids.
#[test]
fn function_packages_are_recorded_with_stable_ids() {
    let mut program = crush_frontend::parse_source(SRC).unwrap();
    for (f, id) in [
        ("helper", "dir:/opt/someone/proj/lib"),
        ("other", "dir:/srv/other/lib"),
        ("third", "pkg:facet"),
        ("main", "root"),
    ] {
        set_package(&mut program.functions.get_mut(f).unwrap().meta, id);
    }
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.expect("owners make a module even without kinds");
    assert!(cosm.kinds.is_empty());
    assert_eq!(cosm.function_package("helper"), Some("dir:lib"));
    assert_eq!(cosm.function_package("other"), Some("dir:lib#2"));
    assert_eq!(cosm.function_package("third"), Some("pkg:facet"));
    assert_eq!(cosm.function_package("main"), None, "root is never written");
    // The lambda lifted out of `helper` is `helper`'s package.
    let lambdas: Vec<&str> = casm
        .functions
        .keys()
        .filter(|f| f.starts_with("__lambda"))
        .map(String::as_str)
        .collect();
    assert!(!lambdas.is_empty(), "{:?}", casm.functions.keys());
    for l in &lambdas {
        assert_eq!(cosm.function_package(l), Some("dir:lib"), "{l}");
    }
    assert_eq!(cosm.fn_packages.len(), 3 + lambdas.len());

    // Nothing of the local directories reaches the artifact.
    let text = String::from_utf8(casm.serialize(casm::Format::Json).unwrap()).unwrap();
    assert!(!text.contains("/opt/someone"), "{text}");
    assert!(!text.contains("/srv/other"), "{text}");
    assert!(text.contains(r#""cosm_version": "0.2.0""#), "{text}");
}

#[test]
fn an_unknown_package_spelling_is_a_compile_error() {
    for bad in ["/abs/lib", "pkg:a/b", "pkg:"] {
        let mut program = crush_frontend::parse_source(SRC).unwrap();
        set_package(&mut program.functions.get_mut("other").unwrap().meta, bad);
        let err = crush_frontend::compile_cast_with_cosm(program)
            .map(|_| ())
            .unwrap_err();
        assert!(format!("{err:#}").contains("package id"), "{bad}: {err:#}");
    }
}

#[test]
fn package_ids_normalize_idempotently() {
    let mut program = crush_frontend::parse_source(SRC).unwrap();
    set_package(
        &mut program.functions.get_mut("helper").unwrap().meta,
        "dir:/a/lib",
    );
    set_package(
        &mut program.functions.get_mut("other").unwrap().meta,
        "dir:/b/lib",
    );
    set_package(
        &mut program.functions.get_mut("third").unwrap().meta,
        "dir:C:\\work\\lib#2",
    );
    crush_frontend::cosm_lower::normalize_package_ids(&mut program).unwrap();
    let ids = |p: &crush_cast::Program| -> Vec<String> {
        ["helper", "other", "third"]
            .iter()
            .map(|f| {
                p.functions[*f].meta["package"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    let once = ids(&program);
    // Distinct directories never share an id, whatever their last component.
    let mut unique = once.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 3, "{once:?}");
    assert!(
        once.iter()
            .all(|id| !id.contains('/') && !id.contains('\\'))
    );
    crush_frontend::cosm_lower::normalize_package_ids(&mut program).unwrap();
    assert_eq!(ids(&program), once);
}

#[test]
fn a_kind_and_its_methods_take_the_declaring_package() {
    let src = "\
struct P { x: Int }
impl P {
    fn get(self) {
        return self.x
    }
}
fn main() {
    let p = P { x: 1 }
    io.print(p.get())
}
";
    let mut program = crush_frontend::parse_source(src).unwrap();
    for f in program.functions.values_mut() {
        for stmt in &mut f.body {
            if let Statement::StructDef { meta, .. } | Statement::ImplBlock { meta, .. } = stmt {
                set_package(meta, "pkg:geom");
                meta.insert("module".to_string(), json!("geo"));
            }
        }
    }
    let (_, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.unwrap();
    let p = cosm.kind("P").unwrap();
    assert_eq!(p.module.as_deref(), Some("geo"));
    assert_eq!(p.package.as_deref(), Some("pkg:geom"));
    assert_eq!(cosm.function_package("P::get"), Some("pkg:geom"));
    assert_eq!(cosm.function_package("main"), None);
}

/// The resolver (V1, #537) stamps `package: "root"` on every merged function.
/// Root is the absence of an owner: the stamp is removed before anything
/// copies meta, so the CASM is the one the same program has without it.
#[test]
fn a_root_package_stamp_changes_nothing() {
    let plain = parse_tree(
        "root-stamp",
        &[("geo.crush", GEO), ("main.crush", GEO_MAIN)],
    );
    let stamped = {
        let mut p = plain.clone();
        for f in p.functions.values_mut() {
            set_package(&mut f.meta, "root");
            for stmt in &mut f.body {
                if let Statement::StructDef { meta, .. }
                | Statement::ImplBlock { meta, .. }
                | Statement::TraitDef { meta, .. } = stmt
                {
                    set_package(meta, "root");
                }
            }
        }
        p
    };
    let a = crush_frontend::compile_cast(&stamped).unwrap();
    let b = crush_frontend::compile_cast(&plain).unwrap();
    for format in [casm::Format::Json, casm::Format::Binary] {
        assert_eq!(document(&a, format), document(&b, format), "{format:?}");
    }
    let text = String::from_utf8(a.serialize(casm::Format::Json).unwrap()).unwrap();
    assert!(!text.contains("\"package\""), "{text}");
    let (_, cosm) = crush_frontend::compile_cast_with_cosm(stamped).unwrap();
    assert!(!cosm.unwrap().has_owners());
}

/// A directory label comes from the spelling alone: `-L .`-style spellings
/// have no real component and are labelled `dir` (`#N` on a clash).
#[test]
fn a_dot_directory_is_labelled_from_its_spelling() {
    let mut program = crush_frontend::parse_source(SRC).unwrap();
    set_package(
        &mut program.functions.get_mut("helper").unwrap().meta,
        "dir:.",
    );
    set_package(
        &mut program.functions.get_mut("other").unwrap().meta,
        "dir:./..",
    );
    set_package(
        &mut program.functions.get_mut("third").unwrap().meta,
        "dir:lib/..",
    );
    crush_frontend::cosm_lower::normalize_package_ids(&mut program).unwrap();
    let id = |f: &str| {
        program.functions[f].meta["package"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut dots = [id("helper"), id("other")];
    dots.sort();
    assert_eq!(dots, ["dir:dir".to_string(), "dir:dir#2".to_string()]);
    assert_eq!(id("third"), "dir:lib");
}

/// The compiler's own owner-id check, on the public `compile_lowered` route
/// that skips `cosm_lower::lower`: a module handed in with a bad id, and a
/// raw (unnormalized) directory on a function.
#[test]
fn compile_lowered_refuses_an_invalid_owner_id() {
    let program = crush_frontend::parse_source("fn main() {\n    io.print(1)\n}\n").unwrap();
    let mut module = crush_cosm::CosmModule::default();
    module
        .fn_packages
        .insert(crush_cosm::FnRef::new("main"), "root".to_string());
    let err = crush_frontend::compiler::Compiler::new()
        .compile_lowered(program.clone(), Some(module))
        .map(|_| ())
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("invalid COSM owner id"),
        "{err:#}"
    );

    let mut raw = program;
    set_package(
        &mut raw.functions.get_mut("main").unwrap().meta,
        "dir:/abs/lib",
    );
    let err = crush_frontend::compiler::Compiler::new()
        .compile_lowered(raw, None)
        .map(|_| ())
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("invalid COSM owner id"),
        "{err:#}"
    );
}

/// Normalization refuses a bad id itself (by `owner_id_problem`), before the
/// compiler's own check would see it.
#[test]
fn normalization_refuses_an_invalid_package_id() {
    for bad in ["/abs/lib", "pkg:a/b", "pkg:", "lib"] {
        let mut program = crush_frontend::parse_source(SRC).unwrap();
        set_package(&mut program.functions.get_mut("other").unwrap().meta, bad);
        let err = crush_frontend::cosm_lower::normalize_package_ids(&mut program).unwrap_err();
        assert!(format!("{err:#}").contains("package id"), "{bad}: {err:#}");
    }
}

/// End to end through the V1 resolver: a library directory (`-L lib`) is its
/// own package. A module of only types there (no function to derive from)
/// is owned through the resolver's stamp on its declarations; a module with
/// functions too. Nothing records the absolute directory.
#[test]
fn a_library_directory_owns_its_types_through_the_resolver() {
    let shapes = "pub struct S { w: Int }\nimpl S {\n    fn width(self) {\n        return self.w\n    }\n}\n";
    let dir = tree(
        "resolver",
        &[
            ("lib/shapes.crush", shapes),
            ("lib/geo.crush", GEO),
            (
                "app/main.crush",
                "import geo\nimport shapes\nfn main() {\n    io.print(geo.make().get())\n}\n",
            ),
        ],
    );
    let options = ImportOptions {
        search_paths: vec![dir.join("lib")],
        ..Default::default()
    };
    let program = crush_frontend::parse_file(&dir.join("app/main.crush"), &options).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.unwrap();
    for (kind, module) in [("S", "shapes"), ("P", "geo")] {
        let k = cosm.kind(kind).unwrap();
        assert_eq!(k.package.as_deref(), Some("dir:lib"), "{kind}");
        assert_eq!(k.module.as_deref(), Some(module), "{kind}");
    }
    assert_eq!(cosm.function_package("S::width"), Some("dir:lib"));
    assert_eq!(cosm.function_package("P::get"), Some("dir:lib"));
    assert_eq!(cosm.function_package("main"), None);
    let text = String::from_utf8(casm.serialize(casm::Format::Json).unwrap()).unwrap();
    assert!(!text.contains(dir.to_str().unwrap()), "absolute dir leaked");
    assert!(!text.contains("\"package\": \"root\""), "root stamp leaked");
}
