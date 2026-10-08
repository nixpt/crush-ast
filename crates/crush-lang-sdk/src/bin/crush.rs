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
//!     crush doctor [--json]  -> report which polyglot runtimes are on PATH
//!
//! Each subcommand's [ARGS] are forwarded verbatim to the underlying binary,
//! so `crush run --help` / `crush build --help` show that tool's own full
//! flag reference rather than a duplicated one here. `doctor` is the one
//! subcommand implemented here (`crush_lang_sdk::doctor`): there is no
//! separate tool for it to dispatch to.

use std::env;
use std::path::PathBuf;
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
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!("crush — the Crush language CLI");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    crush                    start the interactive REPL");
    eprintln!("    crush repl [ARGS]        start the interactive REPL");
    eprintln!("    crush run FILE [ARGS]    run a .crush/.casm/.cvm1 program");
    eprintln!("    crush build FILE [ARGS]  compile a .crush source file to .cvm1");
    eprintln!("    crush doctor [--json]    check the host runtimes polyglot blocks need");
    eprintln!();
    eprintln!("Pass --help after a subcommand for that tool's full flag reference");
    eprintln!("(crush repl --help / crush run --help / crush build --help).");
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
        Some("doctor") => {
            args.next();
            doctor(args)
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
                println!("Report whether python3, node and bash (what `crush run --polyglot` spawns)");
                println!("and the sandbox tools bwrap/buckets are on PATH, with their versions, plus");
                println!("this build's polyglot features. Exits 1 if a required runtime is missing.");
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
    if report.ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
