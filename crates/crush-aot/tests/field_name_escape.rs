//! CRUSH-561: a map-literal key is an arbitrary string literal, and the AOT
//! backends write that key into generated source. Before the fix the Rust
//! backend interpolated it raw, so a key containing `"`, `\` or a newline broke
//! out of the generated Rust string literal and either failed to compile or
//! injected code into the AOT-compiled binary (which runs with no capability
//! gate). This test builds a map whose key carries each dangerous character,
//! reads the value back **by that same key**, and asserts the AOT backends
//! agree with the interpreter.
//!
//! The `"`-in-the-key case (`injection_marker_key_*`) is the one that FAILS on
//! `main`: the generated Rust `insert("a"b".to_string())` does not compile, so
//! `compile_casm` returns an error. Reading the value back by the key also
//! guards against a subtler regression — an escaping that stored the value
//! under the wrong key would make `o[key]` return `null`, not the value.

use crush_aot::AotCompiler;
use std::path::{Path, PathBuf};
use std::process::Command;

fn aot_runner_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_crush-aot-runner"))
}

fn cc_available(cc: &str) -> bool {
    Command::new(cc)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run an AOT shared library and return just the program's printed stdout
/// (the runner appends a final `kind:value` result line, which we drop).
fn run_printed(so: &Path) -> String {
    let output = Command::new(aot_runner_path())
        .arg(so)
        .output()
        .expect("spawn crush-aot-runner");
    assert!(
        output.status.success(),
        "runner exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let body = stdout.strip_suffix('\n').expect("runner result newline");
    match body.rsplit_once('\n') {
        Some((printed, _result)) => format!("{printed}\n"),
        None => String::new(),
    }
}

/// Interpreter (scheduler VM) printed output for the same source.
fn interpreter_printed(source: &str) -> String {
    let report = crush_lang_sdk::differential::differential_run(source)
        .unwrap_or_else(|e| panic!("differential_run failed: {e}"));
    match &report.interpreter {
        crush_lang_sdk::differential::StackOutcome::Ok { output, .. } => output.clone(),
        other => panic!("interpreter did not finish cleanly: {other:?}"),
    }
}

/// A program that stores `7` under `key_literal` (a Crush source string
/// literal, quotes included) and prints the value read back by that same key.
fn program_for(key_literal: &str) -> String {
    format!(
        "fn main() {{\n    let o = {{{key}: 7}}\n    io.print(o[{key}])\n}}\n",
        key = key_literal
    )
}

/// Each case is `(name, key_literal_as_written_in_source)`.
fn cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("double_quote", r#""a\"b""#),
        ("backslash", r#""a\\b""#),
        ("open_brace", r#""a{b""#),
        ("close_brace", r#""a}b""#),
        ("newline", r#""a\nb""#),
        ("non_ascii", r#""café_αβ""#),
    ]
}

#[test]
fn aot_rust_map_key_matches_interpreter() {
    let aot = AotCompiler::new();
    for (name, key) in cases() {
        let source = program_for(key);
        let expected = interpreter_printed(&source);
        assert_eq!(
            expected, "7\n",
            "[{name}] interpreter sanity: value read back by its own key"
        );
        let so = aot
            .compile_casm(
                &crush_frontend::compile_crush_source(&source)
                    .unwrap_or_else(|e| panic!("[{name}] frontend: {e}")),
                &format!("field_escape_rust_{name}"),
            )
            .unwrap_or_else(|e| {
                panic!("[{name}] AOT-Rust compile failed (the CRUSH-561 bug): {e:#}")
            });
        assert_eq!(
            run_printed(&so),
            expected,
            "[{name}] AOT-Rust vs interpreter"
        );
    }
}

#[test]
fn aot_c_map_key_matches_interpreter() {
    if !cc_available("gcc") && !cc_available("clang") {
        eprintln!("no C compiler (gcc/clang) available — skipping AOT-C map-key test");
        return;
    }
    let cc = if cc_available("gcc") { "gcc" } else { "clang" };
    let aot = AotCompiler::new();
    for (name, key) in cases() {
        let source = program_for(key);
        let expected = interpreter_printed(&source);
        let so = aot
            .compile_c(
                &crush_frontend::compile_crush_source(&source)
                    .unwrap_or_else(|e| panic!("[{name}] frontend: {e}")),
                &format!("field_escape_c_{name}"),
                cc,
            )
            .unwrap_or_else(|e| panic!("[{name}] AOT-C compile failed: {e:#}"));
        assert_eq!(run_printed(&so), expected, "[{name}] AOT-C vs interpreter");
    }
}

/// Focused regression for the exact injection shape: a key whose literal form
/// is `"a\"; /* anything */ b"`. On `main` this compiled to broken Rust and
/// failed; the escaped form stores it verbatim and reads it back.
#[test]
fn injection_marker_key_is_escaped_not_executed() {
    let aot = AotCompiler::new();
    // The decoded key contains a double-quote and characters that are Rust
    // syntax; if they reached the generated source unescaped the build breaks.
    let source = program_for(r#""x\"; let INJECTED = 1; \"y""#);
    let expected = interpreter_printed(&source);
    assert_eq!(
        expected, "7\n",
        "interpreter reads the value back by the hostile key"
    );
    let so = aot
        .compile_casm(
            &crush_frontend::compile_crush_source(&source).expect("frontend"),
            "field_escape_injection",
        )
        .expect("AOT-Rust must compile the escaped key (fails on main — CRUSH-561)");
    assert_eq!(
        run_printed(&so),
        expected,
        "AOT-Rust value read back by the hostile key"
    );
}
