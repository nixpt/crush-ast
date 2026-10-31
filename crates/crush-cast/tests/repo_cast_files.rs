//! Every `.cast.json` checked into the repository loads through the gated
//! [`crush_cast::Program::load`] and declares exactly [`CAST_VERSION`] (#365).
//!
//! Before #365 the 20 `benches/fixtures/cast/*.cast.json` declared `1.0.0`
//! and were refused by the major-version gate, while `examples/cast` mixed
//! `0.1` and `0.1.0`.

use std::path::{Path, PathBuf};

use crush_cast::{CAST_VERSION, Program};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".claude") {
                continue;
            }
            collect(&path, out);
        } else if name.ends_with(".cast.json") {
            out.push(path);
        }
    }
}

#[test]
fn every_repo_cast_json_loads_through_the_gate_at_cast_version() {
    let mut files = Vec::new();
    collect(&repo_root(), &mut files);
    files.sort();
    // The corpus is 41 files as of #365; a sweep that finds none is broken.
    assert!(files.len() >= 40, "found only {} .cast.json files", files.len());

    let mut failures = Vec::new();
    for file in &files {
        match Program::load(file) {
            Ok(program) if program.cast_version == CAST_VERSION => {}
            Ok(program) => failures.push(format!(
                "{}: cast_version {:?}, expected {CAST_VERSION:?}",
                file.display(),
                program.cast_version
            )),
            Err(e) => failures.push(format!("{}: {e}", file.display())),
        }
    }
    assert!(failures.is_empty(), "{} of {} failed:\n{}", failures.len(), files.len(), failures.join("\n"));
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests" || n == "target") {
                continue;
            }
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every CAST producer (the Crush parser, the walkers, the codebase walker,
/// the REPL, mutation_check) writes the constant, never a literal: before
/// #365 they wrote `1.0.0`, `0.2` and `"1"`. Test modules may build programs
/// at other versions on purpose (the gate's own tests), so lines after a
/// file's `#[cfg(test)]` are not checked.
#[test]
fn no_producer_writes_a_literal_cast_version() {
    let crates = repo_root().join("crates");
    let mut files = Vec::new();
    for krate in std::fs::read_dir(&crates).unwrap().flatten() {
        collect_rs(&krate.path().join("src"), &mut files);
    }
    let mut offenders = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            let code = line.split("//").next().unwrap_or("");
            if code.contains("cast_version: \"") || code.contains("cast_version=\"") {
                offenders.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(offenders.is_empty(), "write crush_cast::CAST_VERSION instead:\n{}", offenders.join("\n"));
}
