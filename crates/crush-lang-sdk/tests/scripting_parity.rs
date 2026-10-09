//! Crush as a shell-scripting language: everyday shell jobs, each written in bash
//! and in Crush, must print the same thing and exit the same way.
//!
//! `tests/scripting/cases/<name>.sh` and `<name>.crush` do the same job. Each pair
//! runs in its own fresh copy of `tests/scripting/fixture/`, with the same stdin and
//! the same arguments (`one "two words"`); stdout and the exit status are compared.
//! The Crush side runs under `crush-run run --fs --fs-root . --process --env --time`.
//!
//! Known gaps are listed below with the ticket that tracks each. The test fails if an
//! unlisted case differs, and also if a listed one starts matching, so the list
//! can't go stale. Skipped (with a note) where `bash` isn't installed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Cases whose Crush version is known not to match bash yet, and why.
const KNOWN_GAPS: &[(&str, &str)] = &[
    (
        "t03_csv_sum",
        "io.read() returns \"\" for both a blank line and end of input, so the loop stops at the first blank line (CRUSH-239)",
    ),
    (
        "t11_missing_file",
        "a failing capability call can't be caught by try/catch (CRUSH-219)",
    ),
    (
        "t13_count_files",
        "fs.find returns directories too and nothing tells files from directories (CRUSH-240)",
    ),
];

const ARGS: [&str; 2] = ["one", "two words"];

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/scripting")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Stdin for a case: a data file for the cases that read input, otherwise
/// closed (never inherited: an `io.read` on the test's own stdin can block
/// forever).
fn stdin_for(case: &str) -> Stdio {
    let file = match case {
        "t03_csv_sum" => "sales.csv",
        "t09_word_freq" => "words.txt",
        _ => return Stdio::null(),
    };
    Stdio::from(std::fs::File::open(dir().join(file)).unwrap())
}

/// (stdout, exit status) of `cmd` run in a fresh copy of the fixture.
fn run_in_fixture(case: &str, mut cmd: Command) -> (String, Option<i32>) {
    let work = tempfile::tempdir().unwrap();
    copy_dir(&dir().join("fixture"), work.path());
    let out = cmd
        .current_dir(work.path())
        .env_remove("GREETING")
        .stdin(stdin_for(case))
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code())
}

#[test]
fn crush_scripts_do_what_the_bash_scripts_do() {
    if Command::new("bash").arg("--version").output().is_err() {
        eprintln!("bash not found; skipping");
        return;
    }
    let mut cases: Vec<String> = std::fs::read_dir(dir().join("cases"))
        .unwrap()
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension()? == "sh").then(|| p.file_stem().unwrap().to_string_lossy().into_owned())
        })
        .collect();
    cases.sort();
    assert!(cases.len() >= 13, "only {} cases found", cases.len());

    let mut differ = BTreeSet::new();
    let mut report = Vec::new();
    for case in &cases {
        let mut bash = Command::new("bash");
        // `sort` and `find` order by the caller's locale; crush orders by byte. Pin the
        // bash side to the C locale so the comparison doesn't depend on the host's LANG.
        bash.env("LC_ALL", "C")
            .arg(dir().join("cases").join(format!("{case}.sh")))
            .args(ARGS);
        let mut crush = Command::new(env!("CARGO_BIN_EXE_crush-run"));
        crush
            .args(["run", "--fs", "--fs-root", ".", "--process", "--env", "--time"])
            .arg(dir().join("cases").join(format!("{case}.crush")))
            .args(ARGS);
        let expected = run_in_fixture(case, bash);
        let got = run_in_fixture(case, crush);
        if got != expected {
            differ.insert(case.clone());
            report.push(format!(
                "{case}:\n  bash  (exit {:?}): {:?}\n  crush (exit {:?}): {:?}",
                expected.1, expected.0, got.1, got.0
            ));
        }
    }

    let known: BTreeSet<String> = KNOWN_GAPS.iter().map(|(c, _)| c.to_string()).collect();
    let mut failures: Vec<String> = Vec::new();
    for case in differ.difference(&known) {
        let detail = report.iter().find(|r| r.starts_with(&format!("{case}:"))).unwrap();
        failures.push(format!("differs from bash: {detail}"));
    }
    for case in known.difference(&differ) {
        failures.push(format!("{case} now matches bash; remove it from KNOWN_GAPS"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
