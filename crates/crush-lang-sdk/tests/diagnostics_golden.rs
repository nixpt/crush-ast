//! Golden test for `--message-format json` across the Crush CLIs (CRUSH-315).
//!
//! `crushc` and `crush-run` both claim the one seven-field NDJSON schema of
//! `crush-diagnostics`. This runs a corpus of broken programs
//! (`tests/diagnostics_corpus/*.crush`) through both and compares each tool's
//! stderr to a checked-in `*.expected` file:
//!
//! - **compile-time** cases (syntax, semantic): `crushc` and `crush-run` must
//!   emit byte-identical records — same code, same position, same message;
//! - **run-time** cases (a runtime error, an ungranted capability): the
//!   program compiles, so `crushc` must accept it, and only `crush-run` has
//!   something to report.
//!
//! `crush-pkg` / `crush-installer` are not in the matrix: they compile a whole
//! capsule (a manifest plus a *concatenation* of sources) under their own
//! domain codes (`E-BUILDER`, `E-RUN`, ...), so a position would not name a
//! line of any one file.
//!
//! Regenerate after an intended change with `UPDATE_GOLDEN=1 cargo test -p
//! crush-lang-sdk --test diagnostics_golden`, and read the diff.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    /// Fails to compile: both tools report it.
    Compile,
    /// Compiles; `crush-run` fails while running or before it.
    Run,
}

const CASES: &[(&str, Stage)] = &[
    ("parse_expr", Stage::Compile),
    ("undef_var", Stage::Compile),
    ("undef_fn", Stage::Compile),
    ("undef_in_fn", Stage::Compile),
    ("type_binop", Stage::Compile),
    ("type_arg", Stage::Compile),
    ("runtime_index", Stage::Run),
    ("cap_fs", Stage::Run),
];

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostics_corpus")
}

/// Run `bin args` in the corpus directory (so `file` is the bare file name),
/// stdin closed, and return (success, stderr).
fn run(bin: &str, args: &[&str]) -> (bool, String) {
    let out = Command::new(bin)
        .args(args)
        .current_dir(corpus())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn crushc(name: &str, out_dir: &Path) -> (bool, String) {
    let file = format!("{name}.crush");
    let cvm1 = out_dir.join(format!("{name}.cvm1"));
    run(
        env!("CARGO_BIN_EXE_crushc"),
        &[
            "--message-format",
            "json",
            "-o",
            cvm1.to_str().unwrap(),
            &file,
        ],
    )
}

fn crush_run(name: &str) -> (bool, String) {
    let file = format!("{name}.crush");
    run(
        env!("CARGO_BIN_EXE_crush-run"),
        &["run", "--message-format", "json", &file],
    )
}

/// Only the NDJSON records: `crush-run` also prints a `[steps=..]` trailer.
fn records(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| format!("{l}\n"))
        .collect()
}

fn check(name: &str, actual: &str) {
    let path = corpus().join(format!("{name}.expected"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing {}; run with UPDATE_GOLDEN=1", path.display()));
    assert_eq!(
        actual,
        expected,
        "{name}: records differ from {}",
        path.display()
    );
}

#[test]
fn every_cli_emits_the_same_records_for_the_same_broken_program() {
    let out_dir = tempfile::tempdir().unwrap();
    for &(name, stage) in CASES {
        let (c_ok, c_err) = crushc(name, out_dir.path());
        let (r_ok, r_err) = crush_run(name);
        let from_run = records(&r_err);
        assert!(!r_ok, "{name}: crush-run must fail; stderr: {r_err}");
        assert!(
            !from_run.is_empty(),
            "{name}: crush-run emitted no record: {r_err}"
        );
        match stage {
            Stage::Compile => {
                assert!(!c_ok, "{name}: crushc must reject it");
                assert_eq!(
                    records(&c_err),
                    from_run,
                    "{name}: crushc and crush-run disagree about the same program"
                );
            }
            Stage::Run => {
                assert!(
                    c_ok,
                    "{name}: crushc must accept a program that only fails at run time: {c_err}"
                );
                assert_eq!(
                    records(&c_err),
                    "",
                    "{name}: crushc reported something: {c_err}"
                );
            }
        }
        check(name, &from_run);
    }
}

/// Every record, from either tool, has the seven fields in wire order, and a
/// source position whenever the failure is about a place in the source.
#[test]
fn records_follow_the_wire_shape_and_carry_positions() {
    for &(name, stage) in CASES {
        let (_, r_err) = crush_run(name);
        for line in records(&r_err).lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
            // (`serde_json::Value` sorts keys, so check the raw line's order.)
            let fields = ["code", "level", "file", "line", "col", "message", "hint"];
            let at: Vec<usize> = fields
                .iter()
                .map(|f| {
                    line.find(&format!("\"{f}\":"))
                        .unwrap_or_else(|| panic!("{name}: no {f}"))
                })
                .collect();
            assert!(
                at.windows(2).all(|w| w[0] < w[1]),
                "{name}: field order in {line}"
            );
            assert_eq!(
                v.as_object().unwrap().len(),
                fields.len(),
                "{name}: extra fields"
            );
            assert_eq!(v["file"], format!("{name}.crush"), "{name}: file");
            assert!(
                !v["message"].as_str().unwrap().is_empty(),
                "{name}: the text belongs in `message`, not `hint`"
            );
            if stage == Stage::Compile {
                assert!(
                    v["line"].is_u64() && v["col"].is_u64(),
                    "{name}: no position: {line}"
                );
            }
        }
    }
}

/// The runtime and capability codes are the documented ones.
#[test]
fn runtime_and_capability_failures_use_their_own_codes() {
    let code = |name: &str| -> String {
        let (_, err) = crush_run(name);
        let v: serde_json::Value =
            serde_json::from_str(records(&err).lines().next().unwrap()).unwrap();
        v["code"].as_str().unwrap().to_string()
    };
    assert_eq!(code("runtime_index"), "E-RT05");
    assert_eq!(code("cap_fs"), "E-CAPS");
    assert_eq!(code("parse_expr"), "E-PP01");
    assert_eq!(code("undef_var"), "E-TP01");
}
