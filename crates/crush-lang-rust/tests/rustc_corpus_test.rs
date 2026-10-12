//! CRUSH-197: Rust programs must print what `rustc` prints.
//!
//! Each `tests/rustc_corpus/<name>.rs` runs through the walker → CAST → CASM →
//! VM pipeline and its output is compared with `<name>.out`, which was produced
//! by the real `rustc` binary. `expected_outputs_match_rustc` re-derives the
//! `.out` files with `rustc` when one is on PATH, so they can't go stale.

use std::path::{Path, PathBuf};

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rustc_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "empty corpus");
    files
}

fn run_crush(source: &str) -> anyhow::Result<String> {
    let cast = crush_lang_rust::rust_to_cast(source)?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm)?;
    let result = crush_vm::run_with_caps(&vm, &crush_vm::Quotas::default(), None)?;
    Ok(result.output)
}

#[test]
fn crush_output_matches_rustc() {
    let mut failures = Vec::new();
    for path in corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        match run_crush(&source) {
            Ok(got) if got.trim_end() == expected.trim_end() => {}
            Ok(got) => failures.push(format!("{}:\n  rustc: {expected:?}\n  crush: {got:?}", path.display())),
            Err(e) => failures.push(format!("{}: {e:#}", path.display())),
        }
    }
    assert!(failures.is_empty(), "{} program(s) diverge from rustc:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn expected_outputs_match_rustc() {
    let Ok(rustc) = which_rustc() else {
        eprintln!("rustc not on PATH — skipping .out freshness check");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("crush-rustc-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for path in corpus() {
        let bin = tmp.join(path.file_stem().unwrap());
        let ok = std::process::Command::new(&rustc).arg("-O").arg("-o").arg(&bin).arg(&path).status().unwrap();
        assert!(ok.success(), "rustc failed on {}", path.display());
        let out = std::process::Command::new(&bin).output().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{} .out is stale", path.display());
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn which_rustc() -> Result<PathBuf, ()> {
    let probe = std::process::Command::new("rustc").arg("--version").output().map_err(|_| ())?;
    if probe.status.success() { Ok(PathBuf::from("rustc")) } else { Err(()) }
}

#[test]
fn unsupported_format_specs_fail_loudly() {
    let err = crush_lang_rust::rust_to_cast("fn main() { let x = 1.5; println!(\"{:.2}\", x); }").unwrap_err();
    assert!(format!("{err:#}").contains("not supported"), "{err:#}");
}

#[test]
fn unsupported_methods_fail_loudly() {
    let err = crush_lang_rust::rust_to_cast("fn main() { let v = vec![1]; let s: i64 = v.iter().sum(); println!(\"{}\", s); }").unwrap_err();
    assert!(format!("{err:#}").contains(".sum()"), "{err:#}");
}
