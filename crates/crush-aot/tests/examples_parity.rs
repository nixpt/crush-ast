//! Every `examples/crush` program the AOT backends accept must print what the VM prints.
//!
//! Before CRUSH-214/216/223 the Rust backend failed to build 16 of the 27 examples the VM
//! runs, and the C backend garbled six games; no test noticed, because the AOT tests
//! compared only `main`'s return value (CRUSH-220).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crush_aot::AotCompiler;
use crush_lang_sdk::{Quotas, Runtime};

/// `(example, backend)` pairs whose AOT output is known to differ, and why. Keyed by
/// backend so a known divergence on one can't hide a regression on the other. Keep it
/// exact: the test also fails when a pair starts matching, so the list can't go stale.
const KNOWN_DIVERGENCES: &[(&str, &str, &str)] = &[
    ("arrays_and_loops", "rust", "string indexing `s[0]` is null on AOT (CRUSH-217)"),
    ("arrays_and_loops", "c", "string indexing `s[0]` is null on AOT (CRUSH-217)"),
];

/// Parallel builds. Each is a rustc or gcc process of its own, next to the other test
/// binaries `cargo test` runs, so stay well under the core count by default.
fn worker_count() -> usize {
    std::env::var("CRUSH_AOT_PARITY_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()).min(4))
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/crush")
}

fn cc_available(cc: &str) -> bool {
    Command::new(cc).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// What `crush-run` prints, or `None` when the VM can't run the program (it needs a
/// capability grant, fails at run time, or isn't valid Crush).
fn vm_stdout(source: &str) -> Option<String> {
    let program = crush_lang_sdk::compile::compile_crush_source(source).ok()?;
    let quotas = Quotas { max_steps: 20_000_000, ..Quotas::default() };
    let result = Runtime::with_quotas(quotas).run(&program).ok()?;
    result.halted.then_some(result.output)
}

/// Program stdout, without the type-tagged result line `crush-aot-runner` prints last.
fn aot_stdout(so: &Path) -> Result<String, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_crush-aot-runner"))
        .arg(so)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let body = text.trim_end_matches('\n');
    Ok(match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    })
}

#[test]
fn aot_backends_print_what_the_vm_prints_for_every_example() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples/crush")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "crush"))
        .collect();
    files.sort();

    let use_c = cc_available("gcc");
    let compiler = AotCompiler::new();
    let compared = AtomicUsize::new(0);
    let next = AtomicUsize::new(0);
    let diverged = Mutex::new(BTreeSet::new());
    let failures = Mutex::new(Vec::new());

    // Each rustc build takes seconds, so build several examples at once.
    std::thread::scope(|scope| {
        for _ in 0..worker_count() {
            scope.spawn(|| {
                while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                    let source = std::fs::read_to_string(path).unwrap();
                    let Some(expected) = vm_stdout(&source) else { continue };
                    let Ok(program) = crush_frontend::compile_crush_source(&source) else { continue };

                    let mut backends: Vec<(&str, Result<PathBuf, String>)> = vec![(
                        "rust",
                        compiler
                            .compile_casm(&program, &format!("ex_{name}_rust"))
                            .map_err(|e| e.to_string()),
                    )];
                    if use_c {
                        backends.push((
                            "c",
                            compiler
                                .compile_c(&program, &format!("ex_{name}_c"), "gcc")
                                .map_err(|e| e.to_string()),
                        ));
                    }
                    for (backend, so) in backends {
                        let so = match so {
                            Ok(so) => so,
                            // A feature the backend says it doesn't support is not a divergence.
                            Err(e) if e.contains("cannot compile") => continue,
                            Err(e) => {
                                failures.lock().unwrap().push(format!("{name} [{backend}]: build failed: {e}"));
                                continue;
                            }
                        };
                        compared.fetch_add(1, Ordering::Relaxed);
                        match aot_stdout(&so) {
                            Ok(got) if got == expected => {}
                            Ok(_) => {
                                diverged.lock().unwrap().insert((name.clone(), backend.to_string()));
                            }
                            Err(e) => failures.lock().unwrap().push(format!("{name} [{backend}]: run failed: {e}")),
                        }
                    }
                }
            });
        }
    });
    let compared = compared.into_inner();
    let diverged = diverged.into_inner().unwrap();
    let mut failures = failures.into_inner().unwrap();
    failures.sort();

    let known: BTreeSet<(String, String)> = KNOWN_DIVERGENCES
        .iter()
        .filter(|(_, backend, _)| use_c || *backend != "c")
        .map(|(name, backend, _)| (name.to_string(), backend.to_string()))
        .collect();
    for (name, backend) in diverged.difference(&known) {
        failures.push(format!("{name} [{backend}]: AOT output differs from the VM's"));
    }
    for (name, backend) in known.difference(&diverged) {
        failures.push(format!("{name} [{backend}]: now matches the VM; remove it from KNOWN_DIVERGENCES"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Guard against the loop silently skipping everything (e.g. examples moved).
    let floor = if use_c { 36 } else { 18 };
    assert!(compared >= floor, "only {compared} example builds compared");
}
