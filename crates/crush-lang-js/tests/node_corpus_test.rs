//! JS loops must print what Node prints.
//!
//! Each `tests/node_corpus/<name>.js` runs through the walker → CAST → CASM →
//! VM pipeline and is compared with `<name>.out`, produced by `node`.
//! `expected_outputs_match_node` re-derives the `.out` files with `node` when
//! it is on PATH, so they can't go stale.

// The swc path only: the Boa lowering can't run these yet (it has no plain
// `=` assignment lowering; tracked with the Boa frontend work, CRUSH-250).
#![cfg(not(feature = "boa-backend"))]

use std::path::{Path, PathBuf};

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/node_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "js"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "empty corpus");
    files
}

fn run_crush(source: &str) -> anyhow::Result<String> {
    let cast = crush_lang_js::js_to_cast(source, "js")?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    Ok(crush_vm::run_with_caps(&vm, &quotas, None)?.output)
}

#[test]
fn crush_output_matches_node() {
    let mut failures = Vec::new();
    for path in corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        match run_crush(&source) {
            Ok(got) if got.trim_end() == expected.trim_end() => {}
            Ok(got) => failures.push(format!("{}:\n  node:  {expected:?}\n  crush: {got:?}", path.display())),
            Err(e) => failures.push(format!("{}: {e:#}", path.display())),
        }
    }
    assert!(failures.is_empty(), "{} program(s) diverge from node:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn expected_outputs_match_node() {
    if std::process::Command::new("node").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("node not on PATH — skipping .out freshness check");
        return;
    }
    for path in corpus() {
        let out = std::process::Command::new("node").arg(&path).output().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{} .out is stale", path.display());
    }
}

#[test]
fn side_effecting_comma_in_value_position_fails_loudly() {
    let err = crush_lang_js::js_to_cast("var i = 0; var x = (i++, 5); console.log(x);", "js").unwrap_err();
    assert!(format!("{err:#}").contains("comma expression"), "{err:#}");
}
