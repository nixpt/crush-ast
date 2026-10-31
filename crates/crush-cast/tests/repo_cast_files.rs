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
