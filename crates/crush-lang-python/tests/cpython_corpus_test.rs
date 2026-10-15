//! Python e2e parity: programs must print what CPython prints.
//!
//! Each `tests/cpython_corpus/<name>.py` runs through the walker → CAST →
//! CASM → VM pipeline and is compared with `<name>.out`, produced by
//! `python3`. `expected_outputs_match_cpython` re-derives the `.out` files
//! when `python3` is on PATH, so they can't go stale. The AOT backends run
//! the same corpus in `crates/crush-aot/tests/python_parity.rs`.
//!
//! Not covered yet (tracked): closures / lambdas (CRUSH-251), and tuples
//! print with list syntax (they lower to arrays).

use std::path::{Path, PathBuf};

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cpython_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    files.sort();
    assert!(files.len() >= 19, "corpus shrank: {}", files.len());
    files
}

fn run_crush(source: &str) -> anyhow::Result<String> {
    let cast = crush_lang_python::python_to_cast(source)?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
    Ok(crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)?.output)
}

#[test]
fn crush_output_matches_cpython() {
    let mut failures = Vec::new();
    for path in corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        match run_crush(&source) {
            Ok(got) if got == expected => {}
            Ok(got) => failures.push(format!("{}:\n  cpython: {expected:?}\n  crush:   {got:?}", path.display())),
            Err(e) => failures.push(format!("{}: {e:#}", path.display())),
        }
    }
    assert!(failures.is_empty(), "{} program(s) diverge from CPython:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn expected_outputs_match_cpython() {
    if std::process::Command::new("python3").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("python3 not on PATH — skipping .out freshness check");
        return;
    }
    for path in corpus() {
        let out = std::process::Command::new("python3").arg(&path).output().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{} .out is stale", path.display());
    }
}

#[test]
fn lambdas_fail_loudly_until_crush_251() {
    let err = crush_lang_python::python_to_cast("sq = lambda v: v * v\nprint(sq(7))\n").unwrap_err();
    assert!(format!("{err:#}").contains("lambda"), "{err:#}");
}
