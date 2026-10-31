use crush_diagnostics::{DiagRecord, diag_line, diag_line_from, wants_json};
use crush_lint::{AiLinter, lint_source};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "Usage: crush-lint [--json | --message-format=json] <file.crush>";

/// JSON mode: `--json` plus every form `crush_diagnostics::wants_json` knows
/// (`--message-format=json`, `--message-format json`, `--message-format-json`).
fn json_mode(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json") || wants_json(args)
}

/// The file argument: the first positional, skipping the program name and the
/// value of a space-separated `--message-format <fmt>`.
fn file_arg(args: &[String]) -> Option<&str> {
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--message-format" {
            it.next();
        } else if !a.starts_with('-') {
            return Some(a);
        }
    }
    None
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json = json_mode(&args);

    let Some(path) = file_arg(&args).map(PathBuf::from) else {
        if json {
            print!(
                "{}",
                diag_line_from(
                    "E-USAGE",
                    "error",
                    "Missing file argument",
                    Some(USAGE),
                    None
                )
            );
        } else {
            eprintln!("crush-lint — linter for Crush source\n{USAGE}");
        }
        return ExitCode::FAILURE;
    };
    let shown = path.display().to_string();

    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Cannot read {shown}: {e}");
            if json {
                print!(
                    "{}",
                    diag_line_from("E-IO", "error", &msg, None, Some(&shown))
                );
            } else {
                eprintln!("{msg}");
            }
            return ExitCode::FAILURE;
        }
    };

    let linter = AiLinter::new(true);
    let findings = lint_source(&source);
    let mut has_errors = false;

    for f in &findings {
        has_errors |= f.level == "error";
        let context = f
            .line
            .and_then(|l| source.lines().nth(l as usize - 1))
            .unwrap_or("");
        let hint = linter.hint_for(&f.message, context);
        if f.level == "error" {
            linter.log_to_timeline(&shown, &f.message, hint.as_deref().unwrap_or(""));
        }
        if json {
            print!(
                "{}",
                diag_line(&DiagRecord {
                    code: &f.code,
                    level: f.level,
                    file: Some(&shown),
                    line: f.line,
                    col: f.col,
                    message: &f.message,
                    hint: hint.as_deref(),
                })
            );
        } else {
            let at = match f.line {
                Some(l) => format!("{shown}:{l}"),
                None => shown.clone(),
            };
            eprintln!("{}[{}]: {} at {at}", f.level, f.code, f.message);
            if let Some(h) = &hint {
                eprintln!("  hint: {h}");
            }
        }
    }

    if !json && findings.is_empty() {
        println!("{shown} is looking good");
    }
    if has_errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
