//! CRUSH-201: C programs must print what gcc-compiled C prints.
//!
//! Each `tests/gcc_corpus/<name>.c` runs through the walker → CAST → CASM → VM
//! pipeline and is compared with `<name>.out`, produced by the gcc build of
//! the same file. `expected_outputs_match_gcc` re-derives the `.out` files
//! with gcc when it is on PATH, so they can't go stale.

use std::path::{Path, PathBuf};

use crush_lang_c::sdk::{c_to_cast, cast_to_casm, casm_to_vm};

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gcc_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "c"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "empty corpus");
    files
}

fn run_crush(source: &str) -> anyhow::Result<String> {
    let vm = casm_to_vm(&cast_to_casm(&c_to_cast(source, "t.c")?)?)?;
    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    Ok(crush_vm::run_with_caps(&vm, &quotas, None)?.output)
}

#[test]
fn crush_output_matches_gcc() {
    let mut failures = Vec::new();
    for path in corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        match run_crush(&source) {
            Ok(got) if got == expected => {}
            Ok(got) => failures.push(format!("{}:\n  gcc:   {expected:?}\n  crush: {got:?}", path.display())),
            Err(e) => failures.push(format!("{}: {e:#}", path.display())),
        }
    }
    assert!(failures.is_empty(), "{} program(s) diverge from gcc:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn expected_outputs_match_gcc() {
    if std::process::Command::new("gcc").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("gcc not on PATH — skipping .out freshness check");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("crush-gcc-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for path in corpus() {
        let bin = tmp.join(path.file_stem().unwrap());
        let ok = std::process::Command::new("gcc").arg("-w").arg("-o").arg(&bin).arg(&path).status().unwrap();
        assert!(ok.success(), "gcc failed on {}", path.display());
        let out = std::process::Command::new(&bin).output().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{} .out is stale", path.display());
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn lower_err(src: &str) -> String {
    format!("{:#}", c_to_cast(src, "t.c").unwrap_err())
}

#[test]
fn unsupported_printf_conversion_fails_loudly() {
    assert!(lower_err("int main() { double d = 1.5; printf(\"%f\\n\", d); return 0; }").contains("`%f`"));
}

#[test]
fn unsupported_statement_fails_loudly() {
    // `goto` used to be dropped silently.
    let e = lower_err("int main() { int x = 0; goto end; x = 1; end: return x; }");
    assert!(e.contains("not supported"), "{e}");
}

#[test]
fn unsupported_expression_fails_loudly() {
    // `sizeof` used to become null silently.
    let e = lower_err("int main() { int n = sizeof(int); return n; }");
    assert!(e.contains("not supported"), "{e}");
}
