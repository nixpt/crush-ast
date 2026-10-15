//! CRUSH-197: Rust programs must print what `rustc` prints.
//!
//! Each Rust fixture runs through the SDK's public compile API and its output
//! is compared with `<name>.out`, produced by `rustc`.
#![cfg(feature = "polyglot-rust")]

use crush_lang_sdk::{Runtime, compile::compile_rust_source};
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
    let vm = compile_rust_source(source)?;
    let result = Runtime::new().run(&vm)?;
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
            Ok(got) => failures.push(format!(
                "{}:\n  rustc: {expected:?}\n  crush: {got:?}",
                path.display()
            )),
            Err(e) => failures.push(format!("{}: {e:#}", path.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} program(s) diverge from rustc:\n{}",
        failures.len(),
        failures.join("\n")
    );
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
        let ok = std::process::Command::new(&rustc)
            .arg("-O")
            .arg("-o")
            .arg(&bin)
            .arg(&path)
            .status()
            .unwrap();
        assert!(ok.success(), "rustc failed on {}", path.display());
        let out = std::process::Command::new(&bin).output().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            expected,
            "{} .out is stale",
            path.display()
        );
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn which_rustc() -> Result<PathBuf, ()> {
    let probe = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map_err(|_| ())?;
    if probe.status.success() {
        Ok(PathBuf::from("rustc"))
    } else {
        Err(())
    }
}

#[test]
fn unsupported_format_specs_fail_loudly() {
    let err =
        compile_rust_source("fn main() { let x = 1.5; println!(\"{:.2}\", x); }").unwrap_err();
    assert!(format!("{err:#}").contains("not supported"), "{err:#}");
}

#[test]
fn unsupported_methods_fail_loudly() {
    let err = compile_rust_source(
        "fn main() { let v = vec![1]; let s: i64 = v.iter().sum(); println!(\"{}\", s); }",
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains(".sum()"), "{err:#}");
}

#[test]
fn imports_and_qualified_crate_calls_fail_loudly() {
    let import_err =
        compile_rust_source("use std::collections::HashMap; fn main() {}").unwrap_err();
    assert!(
        format!("{import_err:#}").contains("Rust imports and crate APIs"),
        "{import_err:#}"
    );

    let call_err = compile_rust_source("fn main() { std::mem::drop(1); }").unwrap_err();
    assert!(
        format!("{call_err:#}").contains("Rust modules/crates must be exposed"),
        "{call_err:#}"
    );
}
