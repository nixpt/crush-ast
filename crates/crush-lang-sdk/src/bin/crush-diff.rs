//! crush-diff — run crush programs through every linking backend and report divergences.
//!
//!     crush-diff <file.crush | dir>...
//!
//! Restored from the differential intent of exosphere's dropped khukuri crush adapter. Exit 1 if
//! ANY program observably diverges across backends, so it can gate CI.

use crush_lang_sdk::differential::differential_run;
use std::path::{Path, PathBuf};

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(rd) = std::fs::read_dir(path) {
            for e in rd.flatten() {
                collect(&e.path(), out);
            }
        }
    } else if path.extension().is_some_and(|x| x == "crush") {
        out.push(path.to_path_buf());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: crush-diff <file.crush | dir>...");
        std::process::exit(2);
    }

    let mut files = Vec::new();
    for a in &args {
        collect(Path::new(a), &mut files);
    }
    files.sort();

    let (mut diverged, mut compile_err, mut agreed, mut noted) = (0, 0, 0, 0);
    for f in &files {
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("skip {}: {e}", f.display());
                continue;
            }
        };
        match differential_run(&src) {
            Err(e) => {
                compile_err += 1;
                println!("· {}  (does not compile: {e})", f.display());
            }
            Ok(r) if r.diverged() => {
                diverged += 1;
                println!("✗ {}  DIVERGED", f.display());
                for d in &r.divergences {
                    println!("    {d}");
                }
            }
            Ok(r) => {
                agreed += 1;
                if !r.notes.is_empty() {
                    noted += 1;
                    println!("✓ {}  (agrees; {} note(s))", f.display(), r.notes.len());
                    for n in &r.notes {
                        println!("    note: {n}");
                    }
                }
            }
        }
    }

    println!(
        "\n{} files · {agreed} agree ({noted} with notes) · {diverged} DIVERGED · {compile_err} don't compile",
        files.len()
    );
    if diverged > 0 {
        std::process::exit(1);
    }
}
