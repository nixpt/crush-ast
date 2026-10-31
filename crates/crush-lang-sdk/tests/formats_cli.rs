//! The file formats, through the binaries (#365): what each `crushc --emit`
//! writes and where, that every form runs under `crush-run`, the `.cvms`
//! round trip through `crush-compile`, CAST in and out of the CLIs, the CAST
//! file tools, and the one-release deprecation paths. The table these check
//! is `docs/FILE-FORMATS.md`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn bin(name: &str) -> PathBuf {
    let built = match name {
        "crushc" => option_env!("CARGO_BIN_EXE_crushc"),
        "crush-run" => option_env!("CARGO_BIN_EXE_crush-run"),
        "crush-compile" => option_env!("CARGO_BIN_EXE_crush-compile"),
        "crush" => option_env!("CARGO_BIN_EXE_crush"),
        _ => None,
    };
    PathBuf::from(built.unwrap_or(name))
}

fn run(name: &str, dir: &Path, args: &[&str]) -> Output {
    Command::new(bin(name))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("cannot run {name}: {e}"))
}

fn ok(out: &Output, what: &str) -> String {
    assert!(
        out.status.success(),
        "{what} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// `crush-run run FILE` prints exactly `want` on stdout.
fn runs_and_prints(dir: &Path, file: &str, want: &str) -> String {
    let out = run("crush-run", dir, &["run", file]);
    let stdout = ok(&out, &format!("crush-run run {file}"));
    assert_eq!(stdout.trim(), want, "{file}");
    stderr(&out)
}

/// A directory holding `hi.crush`, a program that needs `io.print`.
fn hi() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("hi.crush"), "fn main() { print(40 + 2) }\n").unwrap();
    d
}

#[test]
fn every_artifact_emit_writes_stem_dot_ext_beside_the_input() {
    let d = hi();
    for (emit, file) in [
        ("vm", "hi.cvm1"),
        ("casm", "hi.casm"),
        ("cvms", "hi.cvms"),
        ("cast", "hi.cast.json"),
    ] {
        let out = ok(&run("crushc", d.path(), &["hi.crush", "--emit", emit]), emit);
        let path = d.path().join(file);
        let len = std::fs::metadata(&path).unwrap_or_else(|_| panic!("{file} not written")).len();
        // The path and the real size written are reported.
        assert!(out.contains(&format!("hi.crush → {file}")), "{emit}: {out}");
        assert!(out.contains(&format!("{len} bytes)")), "{emit}: {out} (file is {len} bytes)");
    }
    // The default is vm.
    std::fs::remove_file(d.path().join("hi.cvm1")).unwrap();
    ok(&run("crushc", d.path(), &["hi.crush"]), "crushc");
    assert!(d.path().join("hi.cvm1").is_file());
}

#[test]
fn dash_o_dash_is_stdout_for_every_emit_and_writes_no_file() {
    let d = hi();
    for emit in ["vm", "casm", "cvms", "cast", "ast"] {
        let out = run("crushc", d.path(), &["hi.crush", "--emit", emit, "-o", "-"]);
        assert!(out.status.success(), "{emit}: {}", stderr(&out));
        assert!(!out.stdout.is_empty(), "{emit}: nothing on stdout");
        if emit == "vm" {
            assert!(out.stdout.starts_with(b"CVM1"), "vm: not a CVM1 blob");
        }
        if emit == "casm" || emit == "cast" {
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON on stdout");
            assert!(v.is_object(), "{emit}");
        }
    }
    let files: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(files, ["hi.crush"], "-o - wrote a file");
}

#[test]
fn ast_is_a_stdout_view_and_types_is_a_clear_not_implemented_error() {
    let d = hi();
    let out = ok(&run("crushc", d.path(), &["hi.crush", "--emit", "ast"]), "--emit ast");
    assert!(out.contains("print(40 + 2)"), "{out}");
    let types = run("crushc", d.path(), &["hi.crush", "--emit", "types"]);
    assert!(!types.status.success());
    assert!(stderr(&types).contains("not implemented") && stderr(&types).contains("#371"), "{}", stderr(&types));
    let files: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(files, ["hi.crush"]);
}

#[test]
fn emit_casm_is_casm_json_that_crush_run_runs() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "casm"]), "--emit casm");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(d.path().join("hi.casm")).unwrap()).unwrap();
    assert_eq!(json["version"], casm::CASM_VERSION);
    assert_eq!(json["manifest"]["permissions"], serde_json::json!(["io.print"]));
    // It loads through the gated casm loader.
    casm::Program::load(&d.path().join("hi.casm")).unwrap();
    let err = runs_and_prints(d.path(), "hi.casm", "42");
    assert!(!err.contains("note:"), "a JSON .casm is not deprecated: {err}");
}

#[test]
fn binary_casm_is_refused_rather_than_written_unloadable() {
    let d = hi();
    let out = run("crushc", d.path(), &["hi.crush", "--emit", "casm", "-o", "hi.casmb"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("#372"), "{}", stderr(&out));
    assert!(!d.path().join("hi.casmb").exists());
}

/// The round trip the issue named: a program using `io.print`, emitted as
/// CVM1 assembly, runs under `crush-run`, and `crush-compile` assembles it to
/// a `.cvm1` that runs too — the capability travels as a `.cap` line.
#[test]
fn cvms_round_trips_through_crush_run_and_crush_compile_with_io_print() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "cvms"]), "--emit cvms");
    let text = std::fs::read_to_string(d.path().join("hi.cvms")).unwrap();
    assert!(text.contains(".cap io.print"), "{text}");
    runs_and_prints(d.path(), "hi.cvms", "42");

    let out = ok(&run("crush-compile", d.path(), &["hi.cvms", "-o", "asm.cvm1"]), "crush-compile");
    let len = std::fs::metadata(d.path().join("asm.cvm1")).unwrap().len();
    assert!(out.contains(&format!("({len} bytes)")), "crush-compile must report the bytes written ({len}): {out}");
    runs_and_prints(d.path(), "asm.cvm1", "42");

    // Default output: <stem>.cvm1 beside the input.
    ok(&run("crush-compile", d.path(), &["hi.cvms"]), "crush-compile default");
    runs_and_prints(d.path(), "hi.cvm1", "42");
}

#[test]
fn crush_compile_also_takes_casm_json() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "casm"]), "--emit casm");
    ok(&run("crush-compile", d.path(), &["hi.casm", "-o", "j.cvm1"]), "crush-compile hi.casm");
    runs_and_prints(d.path(), "j.cvm1", "42");
}

#[test]
fn emit_cast_is_cast_json_at_cast_version_that_the_clis_run_and_compile() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "cast"]), "--emit cast");
    let program = crush_cast::Program::load(&d.path().join("hi.cast.json")).unwrap();
    assert_eq!(program.cast_version, crush_cast::CAST_VERSION);
    assert!(program.functions.contains_key("main"));

    runs_and_prints(d.path(), "hi.cast.json", "42");
    // crushc compiles CAST: the artifact is named from the stem without `.cast.json`.
    ok(&run("crushc", d.path(), &["hi.cast.json"]), "crushc hi.cast.json");
    runs_and_prints(d.path(), "hi.cvm1", "42");
    // `.cast` is CAST JSON too (exosphere's name for it).
    std::fs::copy(d.path().join("hi.cast.json"), d.path().join("hi2.cast")).unwrap();
    runs_and_prints(d.path(), "hi2.cast", "42");
    // `crush run` (the umbrella) dispatches to the same runner.
    let out = ok(&run("crush", d.path(), &["run", "hi.cast.json"]), "crush run");
    assert_eq!(out.trim(), "42");
}

#[test]
fn castb_is_written_packed_run_and_compiled() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "cast", "-o", "direct.castb"]), "-o x.castb");
    let p = crush_cast::Program::load(&d.path().join("direct.castb")).unwrap();
    assert_eq!(p.cast_version, crush_cast::CAST_VERSION);
    runs_and_prints(d.path(), "direct.castb", "42");

    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "cast"]), "--emit cast");
    let out = ok(&run("crushc", d.path(), &["cast", "pack", "hi.cast.json"]), "cast pack");
    assert!(out.contains("hi.castb"), "{out}");
    runs_and_prints(d.path(), "hi.castb", "42");
    ok(&run("crushc", d.path(), &["hi.castb", "-o", "b.cvm1"]), "crushc hi.castb");
    runs_and_prints(d.path(), "b.cvm1", "42");
}

#[test]
fn cast_tools_validate_unpack_and_diff() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "cast"]), "--emit cast");
    ok(&run("crushc", d.path(), &["cast", "pack", "hi.cast.json"]), "cast pack");
    for f in ["hi.cast.json", "hi.castb"] {
        let out = ok(&run("crushc", d.path(), &["cast", "validate", f]), "cast validate");
        assert!(out.contains("valid CAST"), "{out}");
    }
    // The umbrella forwards `crush cast …`.
    ok(&run("crush", d.path(), &["cast", "validate", "hi.castb"]), "crush cast validate");

    let out = ok(&run("crushc", d.path(), &["cast", "unpack", "hi.castb", "-o", "back.cast.json"]), "unpack");
    assert!(out.contains("back.cast.json"), "{out}");
    let same = run("crushc", d.path(), &["cast", "diff", "hi.cast.json", "back.cast.json"]);
    assert_eq!(same.status.code(), Some(0), "{}", String::from_utf8_lossy(&same.stdout));

    std::fs::write(d.path().join("other.crush"), "fn main() { print(1) }\n").unwrap();
    ok(&run("crushc", d.path(), &["other.crush", "--emit", "cast"]), "--emit cast other");
    let differ = run("crushc", d.path(), &["cast", "diff", "hi.cast.json", "other.cast.json"]);
    assert_eq!(differ.status.code(), Some(1));

    // A CAST file at another major version is refused by the gate.
    let mut v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(d.path().join("hi.cast.json")).unwrap()).unwrap();
    v["cast_version"] = "1.0.0".into();
    std::fs::write(d.path().join("v1.cast.json"), v.to_string()).unwrap();
    let bad = run("crushc", d.path(), &["cast", "validate", "v1.cast.json"]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(stderr(&bad).contains("version mismatch"), "{}", stderr(&bad));
    let bad = run("crush-run", d.path(), &["run", "v1.cast.json"]);
    assert!(!bad.status.success());
    assert!(stderr(&bad).contains("version mismatch"), "{}", stderr(&bad));
}

#[test]
fn old_names_are_accepted_for_one_release_with_a_note() {
    let d = hi();
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "casm"]), "--emit casm");
    ok(&run("crushc", d.path(), &["hi.crush", "--emit", "cvms"]), "--emit cvms");
    ok(&run("crushc", d.path(), &["hi.crush"]), "crushc");
    std::fs::copy(d.path().join("hi.casm"), d.path().join("old.casm.json")).unwrap();
    std::fs::copy(d.path().join("hi.cvms"), d.path().join("text.casm")).unwrap();
    std::fs::copy(d.path().join("hi.cvm1"), d.path().join("old.cvm")).unwrap();
    std::fs::copy(d.path().join("hi.casm"), d.path().join("sona.sno")).unwrap();

    let err = runs_and_prints(d.path(), "old.casm.json", "42");
    assert!(err.contains("`.casm.json` is deprecated"), "{err}");
    let err = runs_and_prints(d.path(), "text.casm", "42");
    assert!(err.contains("rename it to `.cvms`"), "{err}");
    // A text `.casm` still assembles under crush-compile, with the same note.
    let out = run("crush-compile", d.path(), &["text.casm", "-o", "t.cvm1"]);
    ok(&out, "crush-compile text.casm");
    assert!(stderr(&out).contains("rename it to `.cvms`"), "{}", stderr(&out));
    runs_and_prints(d.path(), "t.cvm1", "42");
    // `.cvm` (crush-pkg's old name) and `.sno` (Sona's CASM JSON) are plain aliases.
    let err = runs_and_prints(d.path(), "old.cvm", "42");
    assert!(!err.contains("note:"), "{err}");
    let err = runs_and_prints(d.path(), "sona.sno", "42");
    assert!(!err.contains("note:"), "{err}");
}

#[test]
fn every_bench_cast_fixture_runs() {
    // The 20 fixtures were refused by the version gate before #365.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benches/fixtures/cast");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.to_string_lossy().ends_with(".cast.json") {
            continue;
        }
        let out = run("crush-run", &dir, &["run", path.to_str().unwrap()]);
        ok(&out, &path.display().to_string());
        n += 1;
    }
    assert_eq!(n, 20);
}

#[test]
fn unknown_extension_names_the_accepted_ones() {
    let d = hi();
    std::fs::write(d.path().join("x.json"), "{}").unwrap();
    let out = run("crush-run", d.path(), &["run", "x.json"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains(".cvms"), "{}", stderr(&out));
}
