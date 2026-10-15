//! CRUSH-197: Rust programs must print what `rustc` prints.
//!
//! Each Rust fixture runs through the SDK's public compile API and its output
//! is compared byte-for-byte with `<name>.out`, produced by real `rustc`.
//! CRUSH-xps-5: unsupported and known-silent fixtures have explicit, pinned
//! ticket-linked policies shared with the AOT parity twin.
#![cfg(all(feature = "polyglot-rust", feature = "stdlib"))]

use crush_lang_sdk::{HostCapsBuilder, Runtime, compile::compile_rust_source};
#[allow(dead_code)]
mod rust_support;
use rust_support::{Engine, Outcome, check, corpus};

fn run_crush(source: &str) -> anyhow::Result<String> {
    let vm = compile_rust_source(source)?;
    let result = Runtime::with_quotas(crush_vm::Quotas {
        max_steps: 500_000,
        ..Default::default()
    })
    .with_host_caps(HostCapsBuilder::new().stdlib(true).build())
    .run(&vm)?;
    Ok(result.output)
}

#[test]
fn rust_vm_conformance_audit() {
    let mut failures = Vec::new();
    for path in corpus() {
        let source = std::fs::read_to_string(&path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let got = match run_crush(&source) {
            Ok(output) => Outcome::Output(output),
            Err(error) => Outcome::Loud(format!("{error:#}")),
        };
        let name = path.file_stem().unwrap().to_str().unwrap();
        if let Err(error) = check(name, Engine::Vm, &expected, got) {
            failures.push(error);
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
fn unsupported_corpus_constructs_report_locations() {
    for path in corpus() {
        let name = path.file_stem().unwrap().to_str().unwrap();
        if matches!(
            rust_support::expectation(name, Engine::Vm),
            rust_support::Expected::Loud { .. }
        ) {
            let source = std::fs::read_to_string(&path).unwrap();
            let error = format!("{:#}", compile_rust_source(&source).unwrap_err());
            // Every lowering rejection includes a one-based line:column,
            // the construct, and its unsupported-feature explanation.
            let has_location = error.split(" at ").skip(1).any(|part| {
                let coordinate = part
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_end_matches([':', ';']);
                coordinate.split_once(':').is_some_and(|(line, col)| {
                    line.parse::<usize>().is_ok_and(|n| n > 0)
                        && col.parse::<usize>().is_ok_and(|n| n > 0)
                })
            });
            assert!(has_location, "{name} missing source location: {error}");
        }
    }
}

#[test]
fn expected_outputs_match_rustc() {
    rust_support::assert_rustc_freshness();
}

#[test]
fn invalid_rust_reports_parse_location() {
    let error = compile_rust_source("fn main() {\n    let = ;\n}").unwrap_err();
    assert!(
        format!("{error:#}").contains("Rust parse error at 2:"),
        "{error:#}"
    );
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
