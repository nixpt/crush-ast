//! Command parser for the interactive REPL front door. The *parser* is
//! real and unit-tested below; the *eval* loop that drives `VmDriver`
//! from parsed `Command`s is `todo!()` until the upstream
//! `crush-vm::PortableVm` breakpoint pause hook lands (see `vm_driver.rs`).

use std::path::PathBuf;

/// One REPL command. The parser only tokenizes; evaluation lives in
/// `session.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `break <file>:<line>` — register a source-level breakpoint.
    Break { file: PathBuf, line: u32 },
    /// `delete <id>` — remove a breakpoint by `BreakpointId`.
    Delete { id: u32 },
    /// `step` — single-step the VM.
    Step,
    /// `continue` — run until the next breakpoint or termination.
    Continue,
    /// `list` — print all registered breakpoints.
    List,
    /// `print <var>` — surface the named local.
    Print { name: String },
    /// `quit` — exit the REPL cleanly.
    Quit,
    /// `help` — print the help banner.
    Help,
    /// `status` — show VM state (instruction count, paused-at breakpoint).
    Status,
}

/// Why a command line didn't parse.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseCommandError {
    /// The line was empty / whitespace-only.
    Empty,
    /// A `break` argument was missing `<file>:<line>` or had a malformed
    /// line number.
    BadBreakpoint(String),
    /// A `delete <id>` had a non-numeric ID.
    BadId(String),
    /// An unknown command verb.
    Unknown(String),
}

impl std::fmt::Display for ParseCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty command"),
            Self::BadBreakpoint(arg) => {
                write!(f, "expected `<file>:<line>` for `break`, got `{arg}`")
            }
            Self::BadId(arg) => write!(f, "expected numeric breakpoint id, got `{arg}`"),
            Self::Unknown(verb) => write!(f, "unknown command `{verb}`"),
        }
    }
}

impl std::error::Error for ParseCommandError {}

/// Parse a `break` argument of the form `<file>:<line>`. Split on the
/// LAST `:` so Windows-style paths like `C:\foo:7` parse as
/// `file=C:\foo` + `line=7`. Reusable by the CLI `--break` flag parser.
pub fn parse_breakpoint_arg(arg: &str) -> Result<(std::path::PathBuf, u32), ParseCommandError> {
    if arg.is_empty() {
        return Err(ParseCommandError::BadBreakpoint(String::new()));
    }
    let (file, line) = arg
        .rsplit_once(':')
        .ok_or_else(|| ParseCommandError::BadBreakpoint(arg.to_string()))?;
    let line: u32 = line
        .parse()
        .map_err(|_| ParseCommandError::BadBreakpoint(arg.to_string()))?;
    Ok((std::path::PathBuf::from(file), line))
}

/// Parse a single REPL input line into a `Command`. Whitespace is
/// trimmed; blank lines return `ParseCommandError::Empty`. Single-letter
/// aliases (`b`, `d`, `s`, `c`, `l`, `p`, `q`, `h`, `?`) are accepted.
/// `break` arguments are split on the LAST `:` so Windows-style paths
/// like `C:\foo:7` parse as `file=C:\foo` + `line=7`.
pub fn parse_command(input: &str) -> Result<Command, ParseCommandError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseCommandError::Empty);
    }
    let mut tokens = trimmed.split_whitespace();
    let verb = tokens.next().expect("non-empty after trim");
    let rest: Vec<&str> = tokens.collect();
    let rest_str = rest.join(" ");
    match verb {
        "break" | "b" => {
            let (file, line) = parse_breakpoint_arg(rest_str.trim())?;
            Ok(Command::Break { file, line })
        }
        "delete" | "d" => {
            let n: u32 = rest_str
                .trim()
                .parse()
                .map_err(|_| ParseCommandError::BadId(rest_str))?;
            Ok(Command::Delete { id: n })
        }
        "step" | "s" => Ok(Command::Step),
        "continue" | "c" => Ok(Command::Continue),
        "list" | "l" => Ok(Command::List),
        "print" | "p" => Ok(Command::Print {
            name: rest_str.trim().to_string(),
        }),
        "quit" | "q" => Ok(Command::Quit),
        "help" | "h" | "?" => Ok(Command::Help),
        "status" | "info" | "i" => Ok(Command::Status),
        other => Err(ParseCommandError::Unknown(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_break_with_default_path_and_line() {
        let cmd = parse_command("break main.crush:7").unwrap();
        assert_eq!(
            cmd,
            Command::Break {
                file: PathBuf::from("main.crush"),
                line: 7,
            }
        );
    }

    /// Path contains only one colon before the line number, exercising
    /// the baseline `rsplit_once` case.
    #[test]
    fn parses_break_with_relative_path_anchoring_on_last_colon() {
        let cmd = parse_command("break ./src/a.crush:42").unwrap();
        assert_eq!(
            cmd,
            Command::Break {
                file: PathBuf::from("./src/a.crush"),
                line: 42,
            }
        );
    }

    /// REGRESSION PIN: the path contains an inner colon BEFORE the
    /// line-number colon (Windows-style `C:\foo`). `rsplit_once` MUST
    /// anchor on the LAST `:` so we get `file=C:\foo` + `line=7` —
    /// NOT `file=C` + `line=\foo:7`. Without this test, a future
    /// `split_once` swap would silently regress all 9 prior tests.
    #[test]
    fn parses_break_with_windows_style_path_anchoring_on_last_colon() {
        let cmd = parse_command(r"break C:\foo:7").unwrap();
        assert_eq!(
            cmd,
            Command::Break {
                file: PathBuf::from(r"C:\foo"),
                line: 7,
            }
        );
    }

    #[test]
    fn rejects_break_without_colon() {
        let err = parse_command("break main.crush").unwrap_err();
        assert!(matches!(err, ParseCommandError::BadBreakpoint(_)));
    }

    #[test]
    fn rejects_break_with_non_numeric_line() {
        let err = parse_command("break main.crush:foo").unwrap_err();
        assert!(matches!(err, ParseCommandError::BadBreakpoint(_)));
    }

    #[test]
    fn parses_all_short_aliases() {
        assert_eq!(parse_command("s").unwrap(), Command::Step);
        assert_eq!(parse_command("c").unwrap(), Command::Continue);
        assert_eq!(parse_command("l").unwrap(), Command::List);
        assert_eq!(parse_command("i").unwrap(), Command::Status);
        assert_eq!(parse_command("q").unwrap(), Command::Quit);
        assert_eq!(parse_command("?").unwrap(), Command::Help);
        assert_eq!(parse_command("h").unwrap(), Command::Help);
    }

    #[test]
    fn parses_status_and_info_aliases() {
        assert_eq!(parse_command("status").unwrap(), Command::Status);
        assert_eq!(parse_command("info").unwrap(), Command::Status);
        assert_eq!(parse_command("i").unwrap(), Command::Status);
    }

    #[test]
    fn parses_delete_id() {
        assert_eq!(parse_command("delete 3").unwrap(), Command::Delete { id: 3 });
        assert_eq!(parse_command("d 12").unwrap(), Command::Delete { id: 12 });
    }

    #[test]
    fn rejects_delete_with_non_numeric_id() {
        let err = parse_command("delete abc").unwrap_err();
        assert!(matches!(err, ParseCommandError::BadId(_)));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse_command("").unwrap_err(), ParseCommandError::Empty);
        assert_eq!(parse_command("   ").unwrap_err(), ParseCommandError::Empty);
    }

    #[test]
    fn rejects_unknown_verb() {
        assert!(matches!(
            parse_command("restart").unwrap_err(),
            ParseCommandError::Unknown(v) if v == "restart"
        ));
    }
}
