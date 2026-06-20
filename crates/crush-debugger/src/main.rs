//! `crush-debugger` binary entry point — clap-driven CLI surface.
//!
//! For the SCAFFOLD commit, only `version` + a stub `run` subcommand are
//! wired. The `run` subcommand currently prints the scaffold flag-set
//! it advertises and exits 0; the `repl` subcommand is the natural place
//! the next iteration moves the in-process REPL behind.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "crush-debugger",
    version,
    about = "Interactive runtime debugger for Crush packages (SCAFFOLD)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print version + scaffold-status banner.
    Version,
    /// Stub: future `-- DebugSession::new(...).run_repl()` live here.
    Run {
        /// Path to a `capsule.toml` or `.cast` target.
        #[arg(value_name = "TARGET")]
        target: String,
        /// Strict mode: downgrade builder `note` -> `error` on display.
        #[arg(long)]
        strict: bool,
    },
    /// Stub: in-process REPL frontend (separate front door from `run`).
    Repl,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Cmd::Version) {
        Cmd::Version => {
            println!(
                "crush-debugger {}\nstatus: SCAFFOLD (initial commit; see lib.rs hook points)",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        Cmd::Run { target, strict } => {
            eprintln!(
                "crush-debugger: `run` is a SCAFFOLD stub.\n  target = {}\n  strict = {}",
                target, strict,
            );
            eprintln!(
                "next step: wire DebugSession::new(PortableVmDriver::new(vm)).run_repl() \
                 once the upstream crush-vm breakpoint hook (portable_vm.rs:1037) lands."
            );
            Ok(())
        }
        Cmd::Repl => {
            eprintln!(
                "crush-debugger: `repl` is a SCAFFOLD stub. \
                 stdin-attached REPL lands alongside the upstream VM hook."
            );
            Ok(())
        }
    }
}
