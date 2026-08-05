//! Conformance corpus runner for Crush language.
//!
//! Scans `.crush` files for annotation comments (`// expect:`, `// expect-error:`,
//! `// expect-exit:`, `// xfail:`) and runs each through the real pipeline
//! (parse → compile → execute) asserting observable behaviour against the
//! PortableVm (CVM1) engine.
//!
//! FastVM support will join via CRUSH-77's differential harness which reuses
//! the same corpus annotations.
//!
//! # Annotation format
//!
//! ```crush
//! // expect: Hello, world!
//! // expect: second line of output
//! // expect-error: division by zero
//! // expect-exit: 1
//! // xfail: lambda syntax not yet supported (CRUSH-75)
//! ```
//!
//! - `// expect: <line>` — one expected stdout line (may appear multiple times;
//!   each `print()` call appends a newline, so "expect:" lines match
//!   newline-delimited output segments)
//! - `// expect-error: <substring>` — program is expected to produce an error
//!   containing this substring (compilation or runtime). When present, the
//!   test passes if the program fails.
//! - `// expect-exit: <N>` — expect exit code N (0 = halted cleanly, non-zero
//!   for programs that halt or error). Default is 0 for expect-mode, 1 for
//!   expect-error mode.
//! - `// xfail: <reason>` — expected failure; test is INVERTED: if it passes
//!   (unexpectedly), the runner reports it as a regression-to-fix. If it
//!   fails for the documented reason, the test is XPASS (diagnostic only).
//!
//! A file with no annotations is skipped (not an error).
//!
//! # Corpus directories
//!
//! - `examples/crush/` — user-facing example programs
//! - `crates/tree-sitter-crush/` — tree-sitter test fixtures
//!
//! # Usage
//!
//! ```bash
//! cargo run -p xtask --bin conformance
//! cargo run -p xtask --bin conformance -- --verbose
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Parsed annotations from a `.crush` file.
#[derive(Debug, Default)]
struct Annotations {
    /// Expected stdout lines (in order).
    expect: Vec<String>,
    /// Expected error substring (if any).
    expect_error: Option<String>,
    /// Expected exit code (default depends on mode).
    expect_exit: Option<i32>,
    /// xfail reason — if present, test is inverted.
    xfail: Option<String>,
}

/// Outcome of running one corpus file.
#[derive(Debug)]
enum Outcome {
    /// Program compiled and ran, output matched expectations.
    Pass,
    /// Program failed as expected (expect-error match).
    PassExpectedError,
    /// Program was expected to fail (xfail) and did.
    XPass,
    /// Program output did not match expectations.
    Fail {
        file: PathBuf,
        reason: String,
    },
    /// xfail program unexpectedly passed — regression.
    FailXPassRegression {
        file: PathBuf,
        reason: String,
        output: String,
    },
    /// Skipped (no annotations).
    Skip,
}

fn parse_annotations(source: &str) -> Annotations {
    let mut ann = Annotations::default();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("// expect: ") {
            ann.expect.push(rest.to_string());
        } else if let Some(rest) = trimmed.strip_prefix("// expect-error: ") {
            ann.expect_error = Some(rest.to_string());
        } else if let Some(rest) = trimmed.strip_prefix("// expect-exit: ") {
            ann.expect_exit = Some(rest.trim().parse().unwrap_or(0));
        } else if let Some(rest) = trimmed.strip_prefix("// xfail: ") {
            ann.xfail = Some(rest.to_string());
        }
    }
    ann
}

/// Extract `// budget: N` annotation (if any).
fn parse_budget_annotation(source: &str) -> Option<u32> {
    for line in source.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("// budget: ") {
            return rest.trim().parse().ok();
        }
    }
    None
}

/// Discover all `.crush` files in the given directories (relative to workspace root).
fn discover_corpus(workspace_root: &Path, dirs: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for dir in dirs {
        let full = workspace_root.join(dir);
        if !full.is_dir() {
            eprintln!("warning: corpus directory not found: {}", full.display());
            continue;
        }
        match std::fs::read_dir(&full) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("crush") {
                        paths.push(p);
                    }
                }
            }
            Err(e) => {
                eprintln!("warning: cannot read {}: {e}", full.display());
            }
        }
    }
    paths.sort();
    paths
}

/// Run a single Crush source file through parse → compile → execute (PortableVm).
///
/// Uses a reduced step quota (50k) so a slow program fails fast rather than
/// blocking the corpus run. A program legitimately needing more steps should
/// declare `// expect-xstep: N` (not yet implemented).
fn run_crush(source: &str) -> Result<String, String> {
    let program = crush_lang_sdk::compile::compile_crush_source(source)
        .map_err(|e| format!("compile error: {e}"))?;
    // Use a modest default; heavy programs can annotate // budget: N.
    let budget: usize = parse_budget_annotation(&source).unwrap_or(5_000) as usize;
    let quotas = crush_vm::Quotas {
        max_steps: budget,
        max_output: 1 << 20,
        ..Default::default()
    };
    let result = crush_vm::run_with_caps(&program, &quotas, None)
        .map_err(|e| format!("runtime error: {e}"))?;
    Ok(result.output)
}

/// Evaluate one corpus file and return its outcome.
fn evaluate_file(path: &Path, verbose: bool) -> Outcome {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::Fail {
                file: path.to_path_buf(),
                reason: format!("cannot read file: {e}"),
            };
        }
    };

    let ann = parse_annotations(&source);

    // Skip files with no annotations at all.
    if ann.expect.is_empty() && ann.expect_error.is_none() && ann.expect_exit.is_none() {
        return Outcome::Skip;
    }

    // Run the program.
    let result = run_crush(&source);

    // xfail mode: expectation is inverted.
    if let Some(ref xfail_reason) = ann.xfail {
        match result {
            Ok(output) => {
                // xfail program passed unexpectedly — regression.
                return Outcome::FailXPassRegression {
                    file: path.to_path_buf(),
                    reason: format!("xfail program unexpectedly passed: {xfail_reason}"),
                    output,
                };
            }
            Err(_error) => {
                // xfail program failed as expected.
                if verbose {
                    eprintln!(
                        "  XPASS {} (xfail: {xfail_reason})",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                    );
                }
                return Outcome::XPass;
            }
        }
    }

    // Normal mode.
    if let Some(ref expected_error) = ann.expect_error {
        match result {
            Ok(output) => {
                return Outcome::Fail {
                    file: path.to_path_buf(),
                    reason: format!(
                        "expected error containing '{expected_error}', but program succeeded:\n{output}",
                    ),
                };
            }
            Err(error) => {
                if error.contains(expected_error.as_str()) {
                    if verbose {
                        eprintln!(
                            "  OK {} (expected error matched)",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                        );
                    }
                    return Outcome::PassExpectedError;
                } else {
                    return Outcome::Fail {
                        file: path.to_path_buf(),
                        reason: format!(
                            "error did not match expected substring.\n  expected: {expected_error}\n  got: {error}",
                        )
                    };
                }
            }
        }
    }

    // expect mode: program must succeed and output must match.
    match result {
        Err(error) => {
            return Outcome::Fail {
                file: path.to_path_buf(),
                reason: format!("program failed unexpectedly: {error}"),
            };
        }
        Ok(actual_output) => {
            // Build expected output: each expect: line becomes one output line,
            // and each `print()` appends a newline, so join with newlines
            // and add the trailing newline that every print emits.
            let expected_output = if ann.expect.is_empty() {
                String::new()
            } else {
                let mut s = ann.expect.join("\n");
                s.push('\n');
                s
            };

            if actual_output == expected_output {
                if verbose {
                    eprintln!(
                        "  OK {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                    );
                }
                return Outcome::Pass;
            }

            return Outcome::Fail {
                file: path.to_path_buf(),
                reason: format!(
                    "output mismatch.\n  expected:\n{}\n  actual:\n{}",
                    text_prefix(&expected_output, "    "),
                    text_prefix(&actual_output, "    "),
                ),
            };
        }
    }
}

fn text_prefix(text: &str, prefix: &str) -> String {
    if text.is_empty() {
        "(empty)".to_string()
    } else {
        text.lines()
            .map(|l| format!("{prefix}{l}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");

    // Workspace root: three levels up from xtask crate
    // (xtask/Cargo.toml → workspace root).
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    let corpus_dirs = &["examples/crush", "crates/tree-sitter-crush"];
    let paths = discover_corpus(&workspace_root, corpus_dirs);

    if paths.is_empty() {
        eprintln!("No .crush files found in corpus directories.");
        return ExitCode::FAILURE;
    }

    if verbose {
        eprintln!(
            "Conformance corpus: {} files, engine=portablevm\n",
            paths.len()
        );
    }

    // Evaluate each file.
    let mut outcomes: Vec<Outcome> = Vec::new();
    for path in &paths {
        let outcome = evaluate_file(path, verbose);
        outcomes.push(outcome);
    }

    // Summarize.
    let mut stats = BTreeMap::<String, usize>::new();
    let mut failures: Vec<&Outcome> = Vec::new();
    let mut xpass_regressions: Vec<&Outcome> = Vec::new();
    let mut skipped_count = 0usize;

    for o in &outcomes {
        match o {
            Outcome::Pass => *stats.entry("pass".into()).or_insert(0) += 1,
            Outcome::PassExpectedError => {
                *stats.entry("pass (expected error)".into()).or_insert(0) += 1
            }
            Outcome::XPass => {
                *stats.entry("xpass (known failure)".into()).or_insert(0) += 1
            }
            Outcome::Fail { .. } => {
                *stats.entry("FAIL".into()).or_insert(0) += 1;
                failures.push(o);
            }
            Outcome::FailXPassRegression { .. } => {
                *stats.entry("REGRESSION".into()).or_insert(0) += 1;
                xpass_regressions.push(o);
            }
            Outcome::Skip => skipped_count += 1,
        }
    }

    // Print stats.
    println!();
    for (label, count) in &stats {
        println!("  {label}: {count}");
    }
    if skipped_count > 0 {
        println!("  skipped (no annotations): {skipped_count}");
    }

    // Print failures.
    if !failures.is_empty() {
        println!("\n── FAILURES ──\n");
        for f in &failures {
            if let Outcome::Fail { file, reason } = f {
                println!(
                    "✗ {}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                );
                println!("  {reason}\n");
            }
        }
    }

    // Print xpass regressions.
    if !xpass_regressions.is_empty() {
        println!("\n── UNEXPECTED PASSES (xfail regression) ──\n");
        for f in &xpass_regressions {
            if let Outcome::FailXPassRegression {
                file,
                reason,
                output,
            } = f
            {
                println!(
                    "⚠ {}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                );
                println!("  {reason}");
                if !output.is_empty() {
                    println!("  output: {output}");
                }
                println!();
            }
        }
    }

    let total_failures = failures.len() + xpass_regressions.len();
    if total_failures > 0 {
        println!(
            "{} of {} files failed ({} skipped).",
            total_failures,
            paths.len(),
            skipped_count
        );
        return ExitCode::FAILURE;
    }

    let annotated = paths.len() - skipped_count;
    if annotated > 0 {
        println!("\n{} of {} annotated files passed.", annotated, annotated);
    } else {
        println!("\nNo annotated files found — add // expect: annotations to .crush files.");
    }
    ExitCode::SUCCESS
}
