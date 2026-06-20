//! Integration tests for the `crush-repl` binary and its
//! `--message-format json` diagnostic mode.
//!
//! Mirrors the existing `crushc_test.rs` / `crush_run_test.rs` /
//! `crush_compile_test.rs` shapes but adapted to the REPL's
//! interactive nature: tests pipe input lines to the REPL via
//! `Stdio::piped()` and append `.quit` as the last line to exit
//! cleanly, then assert on stderr content. JSON records always go to
//! stderr (matching the other three binaries' wire convention); stdout
//! is reserved for prompts + eval results which editors ignore.

use std::io::Write;
use std::process::{Command, Stdio};

fn repl_bin() -> &'static str {
    option_env!("CARGO_BIN_EXE_crush-repl").unwrap_or("crush-repl")
}

/// Spawn the REPL, write trimmed lines + newline to its stdin, then
/// wait for clean exit.
///
/// **REPL-only pattern**: stdout is `Stdio::null()` to avoid the
/// 64KB OS pipe-buffer deadlock with `piped + wait_with_output()`
/// when input scripts grow past 64KB. Assertions live on stderr so
/// dropping stdout capture is invisible. **One-shot binaries
/// (`crushc`/`crush-run`/`crush-compile`) produce bounded stdout** —
/// don't copy this pattern blindly.
fn run_repl_script(args: &[&str], stdin_lines: &[&str]) -> std::process::Output {
    let mut child = Command::new(repl_bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn crush-repl");
    {
        let stdin = child.stdin.as_mut().expect("stdin pipe");
        for line in stdin_lines {
            writeln!(stdin, "{}", line).expect("write to stdin");
        }
        // Drop stdin to send EOF — `.quit` already exits, but a broken
        // input path that hangs the loop will still wake up because the
        // OS closes the pipe.
    }
    child
        .wait_with_output()
        .expect("failed to wait for crush-repl")
}

/// Filter stderr down to the records that look like the `{...}` NDJSON
/// envelope the theme layer emits. Lets the test ignore `[E-PP*]` /
/// `[runtime]` colored badge text that may appear in non-JSON modes or
/// in residual noise from other stderr writes.
fn ndjson_lines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .collect()
}

#[test]
fn crush_repl_emits_json_diagnostic_for_parse_error() {
    // `let = 1` is a single-line, bracket-balanced input that fails
    // parsing: the parser expects an identifier after `let`, sees `=`,
    // emits `UnexpectedToken`. Going through `--message-format json` the
    // REPL's run loop intercepts the typed `Vec<ParseError>` (NOT the
    // flattened anyhow path) and emits one NDJSON record per error via
    // `JsonDiagnostic::parse_error(..)`.
    let output = run_repl_script(
        &["--message-format", "json"],
        &["let = 1", ".quit"],
    );
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json_lines = ndjson_lines(&stderr);
    assert!(
        !json_lines.is_empty(),
        "expected NDJSON record(s) on stderr for parse error, got: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(json_lines[0]).expect("must be valid JSON");
    let code = parsed["code"].as_str().unwrap_or("");
    assert!(
        code.starts_with("E-PP"),
        "expected E-PP* (parse error code), got: {code}"
    );
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(
        parsed["line"].is_number(),
        "parse error must carry line coordinate, got: {parsed}"
    );
    assert!(
        parsed["col"].is_number(),
        "parse error must carry col coordinate, got: {parsed}"
    );
    let msg = parsed["message"].as_str().unwrap_or("");
    assert!(
        !msg.is_empty(),
        "expected non-empty parser message, got: {msg}"
    );
}

#[test]
fn crush_repl_emits_json_diagnostic_for_runtime_error() {
    // `while true { }` is balanced and parses cleanly, then loops
    // forever until the VM step quota trips. With `--max-steps 50` the
    // trip is immediate, surfacing `VmError::StepQuotaExceeded`. The
    // REPL wraps the VmError synthetically in `RuntimeError::Vm` so
    // the JSON dispatch hits `JsonDiagnostic::runtime_error`'s `E-RT05`
    // arm — same code editors see from `crush-run`'s quota-exceeded path.
    let output = run_repl_script(
        &["--message-format", "json", "--max-steps", "50"],
        &["while true { }", ".quit"],
    );
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json_lines = ndjson_lines(&stderr);
    assert!(
        !json_lines.is_empty(),
        "expected NDJSON record on stderr for runtime error, got: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(json_lines[0]).expect("must be valid JSON");
    assert_eq!(
        parsed["code"].as_str(),
        Some("E-RT05"),
        "step-quota runtime errors must surface as E-RT05"
    );
    assert_eq!(parsed["level"].as_str(), Some("error"));
    // Pin the `Vm`-arm shape: `RuntimeError::Vm` carries no source so
    // `JsonDiagnostic::runtime_error` sets `message == ""` and
    // `hint == Some(vm_err.to_string())`. If a future contributor
    // flattens the synthetic wrap or drops the dedupe convention, this
    // assertion will catch it before editors see duplicate-text records.
    assert_eq!(
        parsed["message"].as_str(),
        Some(""),
        "Vm-arm must dedupe VmError display into hint, not message"
    );
    let hint = parsed["hint"].as_str();
    assert!(
        hint.is_some() && !hint.unwrap().is_empty(),
        "Vm-arm hint must carry non-empty VmError display, got: {:?}",
        parsed["hint"]
    );
}

#[test]
fn crush_repl_emits_json_diagnostic_for_meta_command_error() {
    // Meta-command errors with NO typed `ParseError` payload (`.foo` —
    // unrecognized directive) bubble up as flat anyhow::Error from
    // `handle_meta_command`'s `MetaCommandError::Other` arm. The REPL's
    // JSON dispatch routes those to blanket `E-IO` — same convention
    // used by `crush-compile` for its non-assembler errors.
    let output = run_repl_script(
        &["--message-format", "json"],
        &[".bogus", ".quit"],
    );
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json_lines = ndjson_lines(&stderr);
    assert!(
        !json_lines.is_empty(),
        "expected NDJSON record on stderr for meta-command error, got: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(json_lines[0]).expect("must be valid JSON");
    assert_eq!(parsed["code"].as_str(), Some("E-IO"));
    assert!(
        parsed["message"]
            .as_str()
            .unwrap_or("")
            .contains("unknown command"),
        "expected `unknown command` text in message, got: {:?}",
        parsed["message"]
    );
}

#[test]
fn crush_repl_emits_json_diagnostic_for_meta_command_parse_error() {
    // `.type "unterminated` triggers the meta path's parse-error arm:
    // `parse_single_expr` propagates the typed `Vec<ParseError>` with
    // the inner source intact, and the REPL loop's meta-arm emits one
    // NDJSON record per error via `JsonDiagnostic::parse_error`. Same
    // dispatch shape as the top-level eval path.
    let output = run_repl_script(
        &["--message-format", "json"],
        &[".type \"unterminated", ".quit"],
    );
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json_lines = ndjson_lines(&stderr);
    assert!(
        !json_lines.is_empty(),
        "expected NDJSON record on stderr for meta-command parse error, got: {stderr}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(json_lines[0]).expect("must be valid JSON");
    let code = parsed["code"].as_str().unwrap_or("");
    assert!(
        code.starts_with("E-PP"),
        "meta-command parse errors must surface as E-PP* (got {code}); \
         blanket E-IO would mean the typed-Vec propagation regressed"
    );
    assert_eq!(parsed["level"].as_str(), Some("error"));
    assert!(
        parsed["line"].is_number(),
        "parse-error records must carry line coordinate, got: {parsed}"
    );
    assert!(
        parsed["col"].is_number(),
        "parse-error records must carry col coordinate, got: {parsed}"
    );
    let msg = parsed["message"].as_str().unwrap_or("");
    assert!(
        !msg.is_empty(),
        "expected non-empty parser message, got: {msg}"
    );
}

#[test]
fn crush_repl_meta_command_parse_error_default_mode_remains_text() {
    // Parallel lockdown: in default text mode the same meta-command
    // parse error surfaces as a themed `[E-PP*]` badge, NOT as a JSON
    // record. Confirms the text/json split inside `run`'s meta-arm.
    let output = run_repl_script(
        &[],
        &[".type \"unterminated", ".quit"],
    );
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        ndjson_lines(&stderr).is_empty(),
        "default text mode unexpectedly emitted JSON on meta-command parse error: {stderr}"
    );
    assert!(
        stderr.lines().any(|l| l.contains("[E-PP")),
        "expected themed `[E-PP*]` badge in text mode, got: {stderr}"
    );
}

#[test]
fn crush_repl_happy_path_json_mode_emits_no_diagnostic() {
    // Successful literal eval — no NDJSON records on stderr. Guards a
    // regression where a future contributor moves an `eprintln!` outside
    // the dispatch arm and accidentally surfaces a stray
    // `{"code":"E-IO",...}` record on successful evals.
    let output = run_repl_script(
        &["--message-format", "json"],
        &["42", ".quit"],
    );
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        ndjson_lines(&stderr).is_empty(),
        "happy-path JSON mode leaked NDJSON record to stderr: {stderr}"
    );
}

#[test]
fn crush_repl_default_message_format_remains_text() {
    // Default text mode preserves themed `[E-PP*]` badges on stderr so
    // users who don't pass `--message-format` see the same
    // human-readable errors as before this PR.
    let output = run_repl_script(&[], &["let = 1", ".quit"]);
    assert!(output.status.success(), "REPL `.quit` should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        ndjson_lines(&stderr).is_empty(),
        "default text mode unexpectedly emitted JSON, got: {stderr}"
    );
    assert!(
        stderr.lines().any(|l| l.contains("[E-PP")),
        "expected themed `[E-PP*]` badge in text mode, got: {stderr}"
    );
}
