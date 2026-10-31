//! CRUSH-316: crush-lint reports what the real front end reports, and only that.

use crush_lint::lint_source;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_crush-lint"))
        .args(args)
        .current_dir(cwd) // keeps the .dejavue timeline write out of the repo
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-lint")
}

fn sema_file(dir: &Path) -> String {
    let p = dir.join("sema.crush");
    std::fs::write(&p, "let a = 1\nprint(undefined_name)\n").unwrap();
    p.to_string_lossy().into_owned()
}

/// `examples/crush/*.crush` is the conformance corpus. Most programs run;
/// ten carry a `// expect-error:` header because they are known not to
/// (CRUSH-211). The linter must agree with the runner on both groups: zero
/// errors on the programs that run, and the runner's own error on the ones
/// that do not.
#[test]
fn linter_agrees_with_the_example_corpus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/crush");
    let (mut running, mut broken) = (0, 0);
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "crush") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let src = std::fs::read_to_string(&path).unwrap();
        let findings = lint_source(&src);
        let expected = src
            .lines()
            .find_map(|l| l.strip_prefix("// expect-error:"))
            .map(|e| e.trim().trim_start_matches("Parse errors: ").to_string());
        match expected {
            None => {
                running += 1;
                for f in findings.iter().filter(|f| f.level == "error") {
                    problems.push(format!("{name}: false error [{}] {}", f.code, f.message));
                }
            }
            Some(want) => {
                broken += 1;
                // `semantic_switch` (CRUSH-265) fails at codegen, not in the
                // checker; the linter surfaces it as the W-WIP-001 warning.
                let seen = findings.iter().any(|f| f.message.contains(&want))
                    || (want.contains("semantic_switch")
                        && findings.iter().any(|f| f.code == "W-WIP-001"));
                if !seen {
                    problems.push(format!("{name}: expected `{want}`, got {findings:?}"));
                }
            }
        }
    }
    assert!(
        running >= 35 && broken >= 5,
        "corpus not found: {running} running, {broken} broken"
    );
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn no_false_semicolon_error_on_valid_code() {
    let f = lint_source("let a = 1\nlet b = a + 1\nprint(b)\n");
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn undefined_name_is_reported_as_error() {
    let f = lint_source("let a = 1\nprint(undefined_name)\n");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].code, "E-SEMA");
    assert_eq!(f[0].level, "error");
    assert!(f[0].message.contains("undefined_name"));
}

#[test]
fn syntax_error_comes_from_the_parser_with_a_position() {
    let f = lint_source("let = = 1\n");
    assert!(!f.is_empty());
    assert_eq!(f[0].code, "E-SYNTAX");
    assert_eq!(f[0].line, Some(1));
}

#[test]
fn json_flags_emit_only_seven_field_ndjson_on_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let file = sema_file(tmp.path());
    let forms: [&[&str]; 4] = [
        &["--json"],
        &["--message-format", "json"],
        &["--message-format=json"],
        &["--message-format-json"],
    ];
    for form in forms {
        let mut args = form.to_vec();
        args.push(&file);
        let out = run(&args, tmp.path());
        assert_eq!(out.status.code(), Some(1), "{form:?}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        let lines: Vec<_> = stdout.lines().collect();
        assert_eq!(lines.len(), 1, "{form:?}: {stdout}");
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 7, "{form:?}");
        let pos: Vec<_> = ["code", "level", "file", "line", "col", "message", "hint"]
            .iter()
            .map(|k| lines[0].find(&format!("\"{k}\":")).unwrap())
            .collect();
        assert!(
            pos.windows(2).all(|w| w[0] < w[1]),
            "wire field order: {}",
            lines[0]
        );
        assert_eq!(v["code"], "E-SEMA");
        assert_eq!(v["file"], file.as_str());
    }
}

#[test]
fn json_on_clean_file_prints_nothing_and_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("ok.crush");
    std::fs::write(&p, "let a = 1\nprint(a)\n").unwrap();
    let out = run(&["--json", p.to_str().unwrap()], tmp.path());
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn human_output_is_the_default() {
    let tmp = tempfile::tempdir().unwrap();
    let file = sema_file(tmp.path());
    let out = run(&[&file], tmp.path());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("error[E-SEMA]"), "{err}");
    assert!(!err.contains("semicolon"), "{err}");
}
