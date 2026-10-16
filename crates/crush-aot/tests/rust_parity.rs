//! CRUSH-xps-5: the Rust-source corpus on the Rust and C AOT engines.
//! Shared exception policy keeps the SDK and AOT audits in sync.
#[allow(dead_code)]
#[path = "../../crush-lang-sdk/tests/rust_support/mod.rs"]
mod rust_support;

mod common;

use crush_aot::AotCompiler;
use rust_support::{Engine, Outcome, check, corpus, run_bounded};
use std::path::Path;
use std::process::Command;

fn run(so: &Path) -> Outcome {
    let output = run_bounded(Command::new(env!("CARGO_BIN_EXE_crush-aot-runner")).arg(so));
    if !output.status.success() {
        return Outcome::Loud(format!(
            "runner exit {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    assert!(
        output.stderr.is_empty(),
        "unexpected AOT stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Remove exactly the runner's final result line, retaining every byte of
    // program output (including trailing blank lines and whitespace).
    let body = stdout.strip_suffix('\n').expect("runner result newline");
    let (printed, result) = match body.rsplit_once('\n') {
        Some((printed, result)) => (format!("{printed}\n"), result),
        None => (String::new(), body),
    };
    assert!(
        result.starts_with("null:")
            || result.starts_with("int:")
            || result.starts_with("str:")
            || result.starts_with("float:")
            || result.starts_with("bool:"),
        "missing runner result: {result:?}"
    );
    Outcome::Output(printed)
}

fn compile_and_run(engine: Engine, aot: &AotCompiler, name: &str, source: &str) -> Outcome {
    let built = (|| -> anyhow::Result<_> {
        let cast = crush_lang_rust::rust_to_cast(source)?;
        let casm = crush_frontend::compile_cast_owned(cast)?;
        match engine {
            Engine::Rust => aot.compile_casm(&casm, &format!("rust_parity_{name}")),
            Engine::C => aot.compile_c(&casm, &format!("rust_parity_{name}"), "gcc"),
            Engine::Vm => unreachable!(),
        }
    })();
    match built {
        Ok(so) => run(&so),
        Err(error) => Outcome::Loud(format!("{error:#}")),
    }
}

fn audit(engine: Engine) {
    // A private cache unless CI names a shared one (CRUSH_AOT_CACHE_DIR): the cache
    // key covers the generated source, the opt level and the compiler, so a shared
    // cache can't hand back a stale build.
    let cache = tempfile::tempdir().unwrap();
    let aot = if std::env::var_os(crush_aot::compiler::CACHE_DIR_ENV).is_some_and(|d| !d.is_empty()) {
        AotCompiler::new()
    } else {
        AotCompiler::new().with_cache_dir(cache.path().to_path_buf())
    };
    let mut failures = Vec::new();
    let files = corpus();
    // Build and run every program on the worker pool; check the outcomes in order.
    let outcomes = common::par_map(&files, |path| {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let source = std::fs::read_to_string(path).unwrap();
        compile_and_run(engine, &aot, name, &source)
    });
    for (path, outcome) in files.iter().zip(outcomes) {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let expected = std::fs::read_to_string(path.with_extension("out")).unwrap();
        if let Err(error) = check(
            name,
            engine,
            &expected,
            outcome,
        ) {
            failures.push(error);
        }
    }
    assert!(
        failures.is_empty(),
        "Rust parity on {engine:?}:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rust_backend_is_exact_or_ticketed() {
    audit(Engine::Rust);
}

#[test]
fn c_backend_is_exact_or_ticketed() {
    audit(Engine::C);
}
