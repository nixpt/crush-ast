//! Every example under `examples/` must compile — silently-skipped compile failures
//! are what CRUSH-226/#226's review found in `crates/crush-aot/tests/examples_parity.rs`.
//!
//! Unmarked files must compile. Files carrying a `// expect-error: <substring>`
//! marker (first ~20 lines, own line) must *fail* with an error containing
//! `<substring>` — either they fail to compile, or they compile but fail at run
//! time (e.g. `ai_agent_ops.crush`, which exercises AI-native opcodes the VM
//! can't fully execute). If such a file compiles *and* runs cleanly, its marker
//! is stale and must be removed. Failures name the file and the error's first
//! line.

use std::path::{Path, PathBuf};

use crush_lang_sdk::{compile::CompileOptions, effects::used_by, Quotas, Runtime};

/// Compile an example as a file, so its imports resolve from its own
/// directory (CRUSH-110: `examples/crush/imports/`). No `-L`, no `CRUSH_PATH`.
fn compile_example(path: &Path) -> anyhow::Result<crush_vm::Program> {
    crush_lang_sdk::compile::compile_crush_file(
        path,
        &crush_frontend::import_system::ImportOptions::default(),
        &CompileOptions::default(),
    )
}

/// Where the examples live: the workspace root's `examples/` directory.
fn examples_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cursor = manifest.to_path_buf();
    loop {
        let candidate = cursor.join("examples");
        let cargo = cursor.join("Cargo.toml");
        if candidate.is_dir()
            && cargo.is_file()
            && std::fs::read_to_string(&cargo)
                .map(|c| c.contains("[workspace]"))
                .unwrap_or(false)
        {
            return candidate;
        }
        match cursor.parent() {
            Some(p) => cursor = p.to_path_buf(),
            None => break,
        }
    }
    panic!(
        "workspace root with examples/ not found above {}",
        manifest.display()
    );
}

/// All crush source under `examples/` — `examples/**/*.crush`.
fn crush_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_crush(&examples_root(), &mut files);
    files.sort();
    files
}

fn collect_crush(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_crush(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "crush") {
            out.push(path);
        }
    }
}

/// The text of a `// expect-error: <text>` marker, if present in the first ~20
/// lines (own line). Returns the substring after the colon.
fn expect_error_marker(source: &str) -> Option<String> {
    for line in source.lines().take(20) {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("// expect-error:") {
            let text = rest.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// First non-empty line of an error string, trimmed.
fn first_error_line(err: &str) -> String {
    err.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| err.to_string())
}

/// Run a compiled program under the VM, returning its output when it halts.
/// `None` when it can't be compared here (reads input, needs an ungranted
/// capability, or fails at run time).
fn vm_output(program: &crush_vm::Program) -> Option<String> {
    if used_by(program).ok().is_some_and(|used| used.contains("io.read")) {
        return None;
    }
    let quotas = Quotas { max_steps: 20_000_000, ..Quotas::default() };
    match Runtime::with_quotas(quotas).run(program) {
        Ok(result) if result.halted => Some(result.output),
        _ => None,
    }
}

#[test]
fn every_example_compiles_or_has_valid_expect_error_marker() {
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for path in crush_files() {
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("read {}: {e}", path.display())
        });
        let name = path
            .strip_prefix(examples_root())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());

        checked += 1;
        match expect_error_marker(&source) {
            None => {
                // Unmarked: it must compile.
                if let Err(err) = compile_example(&path) {
                    failures.push(format!(
                        "{name}: example fails to compile (no expect-error marker):\n  {}",
                        first_error_line(&err.to_string())
                    ));
                }
            }
            Some(marker) => {
                // Marked: it must fail to compile, or compile but fail at run
                // time, with an error containing <marker>.
                match compile_example(&path) {
                    Err(err) => {
                        let text = err.to_string();
                        if !text.contains(&marker) {
                            failures.push(format!(
                                "{name}: expect-error marker mismatch — compile error does not \
                                 contain {marker:?}:\n  {}",
                                first_error_line(&text)
                            ));
                        }
                    }
                    Ok(program) => match vm_output(&program) {
                        // Compiles and runs cleanly: the marker is stale.
                        Some(_) => {
                            failures.push(format!(
                                "{name}: expect-error marker is stale — compiles and runs \
                                 cleanly but marker expects an error containing {marker:?}"
                            ));
                        }
                        // Fails at run time: the error must contain the marker.
                        None => {
                            // Re-run to capture the error text for the mismatch message.
                            let quotas = Quotas { max_steps: 20_000_000, ..Quotas::default() };
                            let text = match Runtime::with_quotas(quotas).run(&program) {
                                Err(e) => e.to_string(),
                                Ok(_) => "program did not halt but produced no output".to_string(),
                            };
                            if !text.contains(&marker) {
                                failures.push(format!(
                                    "{name}: expect-error marker mismatch — run-time error does \
                                     not contain {marker:?}:\n  {}",
                                    first_error_line(&text)
                                ));
                            }
                        }
                    },
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} example(s) checked, {} compile problem(s):\n{}",
        checked,
        failures.len(),
        failures.join("\n")
    );
}
