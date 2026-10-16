//! Python e2e parity on the AOT backends: **exact or loud**.
//!
//! Every program in `crush-lang-python/tests/cpython_corpus/` is compiled
//! (walker → CASM) by the Rust and C backends. A run that succeeds must print
//! exactly what CPython printed (`<name>.out`); anything else must fail
//! visibly — a compile error or a non-zero exit — never print something
//! different. Each backend may be loud only for the programs in its list
//! (`LOUD_ON_RUST`, `LOUD_ON_C`), and a list fails when an entry starts
//! matching (so it can't go stale).

use std::path::{Path, PathBuf};
use std::process::Command;

use crush_aot::AotCompiler;

mod common;

/// Function values and closures run on the VMs only (CRUSH-251: VM first;
/// the AOT backends refuse make_closure / call_value).
const CLOSURES: &str = "function values / closures are VM-only (CRUSH-251)";

/// Rust backend gaps that must stay loud (compile error), not wrong.
const LOUD_ON_RUST: &[(&str, &str)] = &[("21_lambda", CLOSURES), ("22_closures", CLOSURES)];

/// C backend gaps that must stay loud (compile error), not wrong.
const LOUD_ON_C: &[(&str, &str)] = &[
    ("06_strmeth", "str_split is not implemented in the C runtime"),
    ("18_methods", "str_split is not implemented in the C runtime"),
    ("21_lambda", CLOSURES),
    ("22_closures", CLOSURES),
];

fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crush-lang-python/tests/cpython_corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    files.sort();
    assert!(files.len() >= 19, "corpus shrank: {}", files.len());
    files
}

enum Outcome {
    Exact,
    Loud(String),
    Wrong(String),
}

/// Program stdout without the type-tagged result line the runner prints last.
fn run(so: &Path, expected: &str) -> Outcome {
    let out = Command::new(env!("CARGO_BIN_EXE_crush-aot-runner")).arg(so).output().expect("spawn runner");
    if !out.status.success() {
        return Outcome::Loud(format!("exit {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let body = text.trim_end_matches('\n');
    let printed = match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    };
    if printed == expected { Outcome::Exact } else { Outcome::Wrong(printed) }
}

fn compile_and_run(backend: &str, name: &str, source: &str, expected: &str) -> Outcome {
    let cast = match crush_lang_python::python_to_cast(source) {
        Ok(c) => c,
        Err(e) => return Outcome::Loud(format!("walker: {e}")),
    };
    let casm = match crush_frontend::compiler::Compiler::new().compile(cast) {
        Ok(c) => c,
        Err(e) => return Outcome::Loud(format!("compile: {e}")),
    };
    let aot = AotCompiler::new();
    let built = if backend == "rust" { aot.compile_casm(&casm, &format!("pyp_{name}_r")) } else { aot.compile_c(&casm, &format!("pyp_{name}_c"), "gcc") };
    match built {
        Ok(so) => run(&so, expected),
        Err(e) => Outcome::Loud(format!("aot: {e}")),
    }
}

fn check(backend: &str, allowed_loud: &[(&str, &str)]) {
    let mut problems = Vec::new();
    let files = corpus();
    // Build and run every program on the worker pool; check the outcomes in order.
    let outcomes = common::par_map(&files, |path| {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        compile_and_run(backend, &name, &source, &expected)
    });
    for (path, outcome) in files.iter().zip(outcomes) {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        let may_be_loud = allowed_loud.iter().any(|(n, _)| *n == name);
        match outcome {
            Outcome::Exact if may_be_loud => problems.push(format!("{name}: now exact on {backend} — remove it from the loud list")),
            Outcome::Exact => {}
            Outcome::Loud(why) if may_be_loud => eprintln!("{backend} {name}: loud as expected ({why})"),
            Outcome::Loud(why) => problems.push(format!("{name}: {backend} failed: {why}")),
            Outcome::Wrong(got) => problems.push(format!("{name}: {backend} printed WRONG output:\n  cpython: {expected:?}\n  crush:   {got:?}")),
        }
    }
    assert!(problems.is_empty(), "{} problem(s) on {backend}:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn rust_backend_is_exact_or_loud() {
    check("rust", LOUD_ON_RUST);
}

#[test]
fn c_backend_is_exact_or_loud() {
    if Command::new("gcc").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("gcc not found; skipping the C backend");
        return;
    }
    check("c", LOUD_ON_C);
}
