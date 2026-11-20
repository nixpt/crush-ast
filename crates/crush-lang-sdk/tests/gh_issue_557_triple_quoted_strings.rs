//! #557: a triple-quoted string (`"""..."""`) lexed as `""`, `"hello"`, `""`,
//! so `let s = """hello"""` bound the empty string and the program ran with a
//! wrong answer. Triple-quoted strings are not part of the language (the guide:
//! "Triple-quoted strings are parse errors"); they are now refused at the
//! opening quote, with its position.

use std::process::Command;

const REPRO: &str = "let s = \"\"\"hello\"\"\"\nprint(len(s))\n";

#[test]
fn triple_double_quote_is_a_compile_error_with_position() {
    let err = crush_lang_sdk::compile::compile_crush_source(REPRO)
        .expect_err("triple-quoted strings are refused");
    let msg = format!("{err:#}");
    assert!(msg.contains("triple-quoted strings"), "{msg}");
    assert!(msg.contains("line 1, column 9"), "{msg}");
}

#[test]
fn triple_single_quote_is_a_compile_error() {
    let err = crush_lang_sdk::compile::compile_crush_source("fn main() {\n  print('''hi''')\n}\n")
        .expect_err("triple-quoted strings are refused");
    let msg = format!("{err:#}");
    assert!(msg.contains("triple-quoted strings"), "{msg}");
    assert!(msg.contains("line 2, column 9"), "{msg}");
}

/// End to end through the `crush` CLI: the repro fails loudly, prints nothing
/// on stdout (it used to print `0` and exit 0).
#[test]
fn crush_run_refuses_triple_quoted_string() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("triple.crush");
    std::fs::write(&src, REPRO).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_crush"))
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("failed to execute crush");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.is_empty(), "stdout: {stdout}");
    assert!(stderr.contains("triple-quoted strings"), "stderr: {stderr}");
    assert!(stderr.contains("line 1, column 9"), "stderr: {stderr}");
}

/// The forms next to it still work: an empty string, and a string literal
/// spanning lines (the supported way to write multi-line text).
#[test]
fn crush_run_empty_and_multiline_strings_unaffected() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ok.crush");
    std::fs::write(
        &src,
        "let e = \"\"\nlet m = \"a\nb\"\nprint(len(e))\nprint(m)\nprint('' + \"\\\"\\\"\")\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_crush"))
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("failed to execute crush");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0\na\nb\n\"\"\n");
}
