//! Integration tests for the `crushc` command-line compiler.

use std::process::Command;

fn crushc_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crushc").unwrap_or("crushc")
}

fn crush_run_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-run").unwrap_or("crush-run")
}

fn run_crushc(args: &[&str]) -> std::process::Output {
    Command::new(crushc_bin())
        .args(args)
        .output()
        .expect("failed to execute crushc")
}

#[test]
fn crushc_compiles_to_cvm1_default_output() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("hello.crush");
    let out = dir.path().join("hello.cvm1");
    std::fs::write(&src, "fn main() { io.print(\"hello\") }").unwrap();

    let output = run_crushc(&[src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.exists(), "expected default output file to be created");
}

#[test]
fn crushc_check_valid_program() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ok.crush");
    std::fs::write(&src, "fn main() { io.print(\"ok\") }").unwrap();

    let output = run_crushc(&["--check", src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no errors detected"), "stderr: {stderr}");
}

#[test]
fn crushc_reports_type_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    std::fs::write(&src, "fn main() { let x = true + 1 }").unwrap();

    let output = run_crushc(&[src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Post-readability pass the CLI uses a `[type]` badge for type-check
    // errors; the inner message still describes the underlying mismatch.
    assert!(
        stderr.contains("[type]") && stderr.contains("Invalid binary op +"),
        "stderr: {stderr}"
    );
}

#[test]
fn crushc_emits_json_diagnostic_for_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    // Unterminated curly — produces exactly one parse error.
    std::fs::write(&src, "fn main() {").unwrap();

    let output = run_crushc(&["--message-format", "json", src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // One NDJSON record on stderr.
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert!(parsed["code"].is_string(), "code: {}", parsed);
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(parsed["file"].is_string());
    assert!(parsed["line"].as_u64().is_some());
    assert!(parsed["col"].as_u64().is_some());
    assert!(parsed["message"].is_string());
    assert!(parsed["hint"].is_null() || parsed["hint"].is_string());
}

#[test]
fn crushc_emits_json_diagnostic_for_type_error() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    std::fs::write(&src, "fn main() { let x = true + 1 }").unwrap();

    let output = run_crushc(&["--message-format", "json", src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-TP01"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(parsed["file"].is_string());
    // CRUSH-315: semantic errors carry the position of the offending
    // operator (`+`, line 1, column 26 of the one-line source).
    assert_eq!(parsed["line"].as_u64(), Some(1));
    assert_eq!(parsed["col"].as_u64(), Some(26));
    assert!(parsed["message"].as_str().unwrap_or("").contains("binary"));
}

#[test]
fn crushc_default_message_format_remains_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bad.crush");
    std::fs::write(&src, "fn main() { let x = true + 1 }").unwrap();

    let output = run_crushc(&[src.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Default is text — must not emit raw JSON to stderr.
    assert!(
        !stderr.trim_start().starts_with('{'),
        "default mode unexpectedly emitted JSON: {stderr}"
    );
    assert!(stderr.contains("[type]"), "stderr: {stderr}");
}

#[test]
fn crushc_emits_json_diagnostic_for_io_error() {
    // --message-format json covers non-themed failures (file-not-found,
    // unknown emit kind, etc.) so editors get a uniform NDJSON stream.
    let output = run_crushc(&[
        "--message-format",
        "json",
        "/nonexistent/crush-ast-path/missing.crush",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.lines().filter(|l| !l.is_empty()).count(),
        1,
        "expected exactly one NDJSON record, got stderr: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stderr.lines().next().unwrap()).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-IO"));
    assert_eq!(parsed["level"].as_str(), Some("error"));
    // IO failures name the file but carry no source coordinates.
    assert!(parsed["file"].as_str().is_some_and(|f| f.ends_with("missing.crush")));
    assert!(parsed["line"].is_null());
    assert!(parsed["col"].is_null());
    assert!(
        parsed["message"]
            .as_str()
            .unwrap_or("")
            .contains("No such file")
            || parsed["message"]
                .as_str()
                .unwrap_or("")
                .contains("cannot read"),
        "expected human-readable IO message, got: {}",
        parsed["message"]
    );
}

#[test]
fn crushc_default_message_format_remains_text_for_io_error() {
    // Symmetry check: default text mode on a missing file still emits
    // a `crushc: ...` prefix line and not JSON.
    let output = run_crushc(&["/nonexistent/crush-ast-path/missing.crush"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim_start().starts_with('{'),
        "default mode unexpectedly emitted JSON: {stderr}"
    );
    assert!(
        stderr.contains("crushc:"),
        "expected `crushc:` prefix in default text mode, got: {stderr}"
    );
}

#[test]
fn crushc_emits_casm() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("emit.crush");
    std::fs::write(&src, "fn main() { io.print(\"hi\") }").unwrap();

    // CVM1 assembly (`--emit cvms`, #365; this was `--emit casm` before).
    let output = run_crushc(&["--emit", "cvms", "-o", "-", src.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(".func main"), "stdout: {stdout}");
    assert!(stdout.contains(".cap io.print"), "stdout: {stdout}");

    // `--emit casm` is CASM JSON now, written beside the input.
    let output = run_crushc(&["--emit", "casm", src.to_str().unwrap()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("emit.casm")).unwrap()).unwrap();
    assert_eq!(json["manifest"]["permissions"][0], "io.print");
    assert!(json["functions"]["main"]["body"].is_array(), "{json}");
}

#[test]
fn crushc_compiled_program_runs() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("run.crush");
    let out = dir.path().join("run.cvm1");
    std::fs::write(&src, "fn main() { io.print(\"compiled\") }").unwrap();

    let compile = run_crushc(&["-o", out.to_str().unwrap(), src.to_str().unwrap()]);
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    assert!(out.exists());

    let run = Command::new(crush_run_bin())
        .args(["run", out.to_str().unwrap()])
        .output()
        .expect("failed to execute crush-run");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("compiled"), "stdout: {stdout}");
}

// #260: `Acc(10)` of a `@kind(construct: ctor(init))` Kind is a constructor
// call. crushc used to type-check without the constructors declared and
// reject it ("Undefined function: Acc") while crush-run ran it; with and
// without -O it now compiles, and the program runs like `crush-run x.crush`.
#[test]
fn crushc_compiles_a_constructed_kind_call() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ctor.crush");
    std::fs::write(
        &src,
        "@kind(construct: ctor(init))\nstruct Acc { total: Int }\nimpl Acc {\n    fn init(self, start) { self.total = start }\n    fn add(self, n) {\n        self.total = self.total + n\n        return self\n    }\n}\nlet a = Acc(10)\na.add(5)\nprint(a.total)\n",
    )
    .unwrap();
    for opt in [false, true] {
        let out = dir.path().join(format!("ctor{opt}.cvm1"));
        let mut args = vec!["-o", out.to_str().unwrap(), src.to_str().unwrap()];
        if opt {
            args.insert(0, "-O");
        }
        let compile = run_crushc(&args);
        assert!(compile.status.success(), "-O={opt}: {}", String::from_utf8_lossy(&compile.stderr));
        let run = Command::new(crush_run_bin())
            .args(["run", out.to_str().unwrap()])
            .output()
            .expect("failed to execute crush-run");
        assert!(run.status.success(), "-O={opt}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "15", "-O={opt}");
    }
    let casm = run_crushc(&["-O", "--emit", "cvms", "-o", "-", src.to_str().unwrap()]);
    assert!(casm.status.success(), "{}", String::from_utf8_lossy(&casm.stderr));
    assert!(String::from_utf8_lossy(&casm.stdout).contains("CONSTRUCT \"Acc\" 1"));
}
