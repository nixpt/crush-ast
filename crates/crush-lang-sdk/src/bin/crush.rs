//! `crush` — the umbrella CLI for the Crush language.
//!
//! A thin arg-dispatcher over the tools that already ship in this crate —
//! it adds no new logic, just names the existing capability the way a
//! stranger running `cargo install crush-lang-sdk` would expect to find it:
//!
//!     crush                  -> REPL       (crush-repl)
//!     crush repl [ARGS]      -> REPL       (crush-repl [ARGS])
//!     crush run FILE [ARGS]  -> run        (crush-run run FILE [ARGS])
//!     crush build FILE [ARGS]-> compile    (crushc FILE [ARGS])
//!     crush debug FILE [ARGS]-> debugger   (crush-debug FILE [ARGS])
//!     crush gui run FILE --ui-> GUI runner (crush-gui run FILE --ui; CRUSH-319)
//!     crush doctor [--json]  -> report which polyglot runtimes are on PATH
//!     crush update [ARGS]    -> self-update (crush-installer update [ARGS]; CRUSH-344)
//!
//! Each subcommand's [ARGS] are forwarded verbatim to the underlying binary,
//! so `crush run --help` / `crush build --help` show that tool's own full
//! flag reference rather than a duplicated one here. `doctor` is the one
//! subcommand implemented here (`crush_lang_sdk::doctor`): there is no
//! separate tool for it to dispatch to.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Resolve a sibling binary next to the currently-running `crush` exe.
/// `cargo install` / `cargo build` place every `[[bin]]` of a package in the
/// same directory, so this holds for both a dev build and an installed one.
fn sibling(name: &str) -> PathBuf {
    let mut path = env::current_exe().expect("crush: could not resolve its own executable path");
    path.set_file_name(name);
    path
}

fn exec(name: &str, args: impl IntoIterator<Item = String>) -> ExitCode {
    let bin = sibling(name);
    match Command::new(&bin).args(args).status() {
        Ok(status) => match status.code() {
            Some(code) => ExitCode::from(code as u8),
            None => ExitCode::FAILURE, // killed by signal
        },
        Err(e) => {
            eprintln!("crush: failed to launch '{}': {e}", bin.display());
            if name == "crush-debug" {
                eprintln!("(crush-debug is built with the `debug` feature of crush-lang-sdk)");
            }
            if name == "crush-gui" {
                eprintln!("(crush-gui is its own package: `cargo install crush-gui`, or build the workspace)");
            }
            ExitCode::FAILURE
        }
    }
}

/// `crush update` → `crush-installer update`. The installer is found next to
/// `crush` (an installer-managed install) or on `PATH` (`cargo install
/// crush-installer`); it is told where this `crush` lives so it updates *this*
/// toolchain — and refuses, rather than updating some other copy, when this one
/// is cargo's.
fn update(args: impl Iterator<Item = String>) -> ExitCode {
    let installer = "crush-installer";
    let sibling = sibling(installer);
    let bin = if sibling.is_file() {
        Some(sibling)
    } else {
        find_on_path(installer, env::var_os("PATH").as_deref())
    };
    let Some(bin) = bin else {
        eprintln!("crush: `crush update` needs crush-installer, which is not next to crush or on PATH.");
        eprintln!("Install it with: cargo install crush-installer");
        eprintln!("(or, if this crush came from cargo, update it with: cargo install crush-lang-sdk)");
        return ExitCode::FAILURE;
    };
    let me = env::current_exe().ok();
    let caller_dir = me.as_deref().and_then(Path::parent);
    match Command::new(&bin).args(installer_args(caller_dir, args)).status() {
        Ok(status) => match status.code() {
            Some(code) => ExitCode::from(code as u8),
            None => ExitCode::FAILURE,
        },
        Err(e) => {
            eprintln!("crush: failed to launch '{}': {e}", bin.display());
            ExitCode::FAILURE
        }
    }
}

fn installer_args(caller_dir: Option<&Path>, args: impl Iterator<Item = String>) -> Vec<String> {
    let mut v = vec!["update".to_string()];
    if let Some(d) = caller_dir {
        v.push("--caller-dir".to_string());
        v.push(d.display().to_string());
    }
    v.extend(args);
    v
}

fn find_on_path(name: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    env::split_paths(path?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

fn usage() {
    eprintln!("crush — the Crush language CLI");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    crush                    start the interactive REPL");
    eprintln!("    crush repl [ARGS]        start the interactive REPL");
    eprintln!("    crush run FILE [ARGS]    run a .crush/.casm/.cvm1 program");
    eprintln!("    crush build FILE [ARGS]  compile a .crush source file to .cvm1");
    eprintln!("    crush debug FILE [ARGS]  debug a .crush program: break, step, watch, print");
    eprintln!("    crush gui run FILE --ui  run a program that draws a window with ui.* (needs crush-gui)");
    eprintln!("    crush doctor [--json]    check the host runtimes polyglot blocks need");
    eprintln!("    crush update [--check]   update to the newest release (needs crush-installer)");
    eprintln!();
    eprintln!("Pass --help after a subcommand for that tool's full flag reference");
    eprintln!("(crush repl --help / crush run --help / crush build --help / crush debug --help).");
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1).peekable();

    match args.peek().map(String::as_str) {
        None => exec("crush-repl", args),
        Some("repl") => {
            args.next();
            exec("crush-repl", args)
        }
        Some("run") => {
            args.next();
            exec("crush-run", std::iter::once("run".to_string()).chain(args))
        }
        Some("build") => {
            args.next();
            exec("crushc", args)
        }
        Some("debug") => {
            args.next();
            exec("crush-debug", args)
        }
        Some("gui") => {
            args.next();
            exec("crush-gui", args)
        }
        Some("doctor") => {
            args.next();
            doctor(args)
        }
        Some("update") => {
            args.next();
            update(args)
        }
        Some("-h") | Some("--help") => {
            usage();
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            println!("crush {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("crush: unknown subcommand '{other}'\n");
            usage();
            ExitCode::FAILURE
        }
    }
}

fn doctor(args: impl Iterator<Item = String>) -> ExitCode {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("crush doctor [--json]");
                println!();
                println!(
                    "Report whether python3, node and bash (what `crush run --polyglot` spawns)"
                );
                println!(
                    "and the sandbox tools bwrap/buckets are on PATH, with their versions, plus"
                );
                println!(
                    "this build's polyglot features. Exits 1 if a required runtime is missing."
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("crush doctor: unknown argument '{other}' (try --json)");
                return ExitCode::from(2);
            }
        }
    }
    let report = crush_lang_sdk::doctor::Report::collect(None);
    if json {
        println!("{}", report.to_json());
    } else {
        print!("{}", report.to_text());
    }
    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_forwards_args_after_the_caller_dir() {
        let got = installer_args(
            Some(Path::new("/opt/crush/bin")),
            ["--check".to_string()].into_iter(),
        );
        assert_eq!(got, ["update", "--caller-dir", "/opt/crush/bin", "--check"]);
        assert_eq!(installer_args(None, std::iter::empty()), ["update"]);
    }

    #[test]
    fn find_on_path_returns_the_first_match_and_none_when_absent() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(b.path().join("crush-installer"), "").unwrap();
        let path = env::join_paths([a.path(), b.path()]).unwrap();
        assert_eq!(
            find_on_path("crush-installer", Some(&path)),
            Some(b.path().join("crush-installer"))
        );
        assert_eq!(find_on_path("nope", Some(&path)), None);
        assert_eq!(find_on_path("crush-installer", None), None);
    }
}
