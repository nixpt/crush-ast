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

#[test]
fn a_kind_from_an_imported_module_records_its_module() {
    let dir = tree(
        "module",
        &[
            (
                "geo.crush",
                "struct P { x: Int }\nimpl P {\n    fn get(self) {\n        return self.x\n    }\n}\nfn make() {\n    return P { x: 1 }\n}\n",
            ),
            (
                "main.crush",
                "import geo\nstruct Q { y: Int }\nfn main() {\n    io.print(geo.make())\n}\n",
            ),
        ],
    );
    let program =
        crush_frontend::parse_file(&dir.join("main.crush"), &ImportOptions::default()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let (casm, cosm) = crush_frontend::compile_cast_with_cosm(program).unwrap();
    let cosm = cosm.expect("kinds");
    assert_eq!(casm.cosm.as_ref(), Some(&cosm));
    let p = cosm.kind("P").unwrap();
    assert_eq!(p.module.as_deref(), Some("geo"));
    assert_eq!(p.package, None, "no package source on main yet (V1, #519)");
    let q = cosm.kind("Q").unwrap();
    assert_eq!((q.module.as_deref(), q.package.as_deref()), (None, None));
    assert_eq!(cosm.required_version(), crush_cosm::COSM_OWNERS_VERSION);
    assert!(cosm.fn_packages.is_empty());
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
        ("helper", "dir:/home/someone/proj/lib"),
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
    assert!(!text.contains("/home/someone"), "{text}");
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
            .map(|f| p.functions[*f].meta["package"].as_str().unwrap().to_string())
            .collect()
    };
    let once = ids(&program);
    // Distinct directories never share an id, whatever their last component.
    let mut unique = once.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 3, "{once:?}");
    assert!(once.iter().all(|id| !id.contains('/') && !id.contains('\\')));
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
