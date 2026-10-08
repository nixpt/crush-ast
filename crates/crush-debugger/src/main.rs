//! `crush-debugger` binary entry point — clap-driven CLI surface.
//!
//! The `run` subcommand loads a `.crush` assembly file, compiles it
//! via `crush_vm::assemble`, creates a `PortableVm` + driver + debug
//! session, and drops into the interactive REPL.
//!
//! Pass `--cap <NAME>` for each capability the target program needs
//! (e.g. `--cap io.print`).

use clap::{Args, Parser, Subcommand};

use crush_debugger::{DebugSession, PortableVmDriver, repl};

#[derive(Parser, Debug)]
#[command(
    name = "crush-debugger",
    version,
    about = "Interactive runtime debugger for Crush packages"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print version.
    Version,
    /// Load a `.crush` assembly file and start the interactive REPL.
    Run(RunArgs),
    /// Stub: in-process REPL frontend (separate front door from `run`).
    Repl,
}

#[derive(Args, Debug)]
struct RunArgs {
    /// Path to a `.crush` assembly source file.
    #[arg(value_name = "TARGET")]
    target: String,

    /// Grant a capability (repeatable). Pass once per capability
    /// the target program declares, e.g. `--cap io.print`.
    ///
    /// `debug.*` names grant the debugger itself, not the program:
    /// `debug.step` to break/step/continue/watch, plus
    /// `debug.inspect.redacted` (values as type + hash) or `debug.inspect`
    /// (values in full) to `print` and see values in stops. Without
    /// `debug.step` the REPL refuses every command that runs the program.
    #[arg(long = "cap", value_name = "NAME")]
    capabilities: Vec<String>,

    /// Set a breakpoint before entering the REPL (repeatable).
    /// Format: `--break <FILE>:<LINE>`.
    #[arg(long = "break", value_name = "FILE:LINE")]
    breakpoints: Vec<String>,

    /// Step quota for `continue`. Prevents long-running capsules
    /// from hanging the debugger. Defaults to the VM's built-in
    /// quota (1M steps) if not set.
    #[arg(long = "max-steps", value_name = "N")]
    max_steps: Option<usize>,

    /// Stack depth quota. Execution errors with `StackQuota` when
    /// the value stack exceeds this many entries.
    #[arg(long = "max-stack", value_name = "N")]
    max_stack: Option<usize>,

    /// Output byte quota. Execution errors with `OutputQuota` when
    /// total printed output exceeds this many bytes.
    #[arg(long = "max-output", value_name = "N")]
    max_output: Option<usize>,

    /// Call depth quota. Execution errors with `CallDepthQuota` when
    /// the call stack exceeds this many frames.
    #[arg(long = "max-call-depth", value_name = "N")]
    max_call_depth: Option<usize>,

    /// Strict mode: unused (reserved).
    #[arg(long)]
    strict: bool,
}

/// The debugger grants named by `--cap debug.*` flags.
fn debug_visibility(names: &[&str]) -> anyhow::Result<crush_vm::DebugVisibility> {
    use crush_vm::DebugVisibility as V;
    use crush_vm::debug::{DEBUG_INSPECT, DEBUG_INSPECT_REDACTED, DEBUG_STEP};
    for &name in names {
        if ![DEBUG_STEP, DEBUG_INSPECT_REDACTED, DEBUG_INSPECT].contains(&name) {
            anyhow::bail!(
                "unknown debugger grant `{name}` (expected {DEBUG_STEP}, \
                 {DEBUG_INSPECT_REDACTED} or {DEBUG_INSPECT})"
            );
        }
    }
    let has = |n| names.contains(&n);
    if !has(DEBUG_STEP) {
        if !names.is_empty() {
            anyhow::bail!("`{}` needs `--cap {DEBUG_STEP}` too", names[0]);
        }
        return Ok(V::None);
    }
    Ok(if has(DEBUG_INSPECT) {
        V::Full
    } else if has(DEBUG_INSPECT_REDACTED) {
        V::InspectRedacted
    } else {
        V::ControlOnly
    })
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Cmd::Version) {
        Cmd::Version => {
            println!("crush-debugger {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Cmd::Run(args) => {
            let _ = args.strict;
            let source = std::fs::read_to_string(&args.target)
                .map_err(|e| anyhow::anyhow!("cannot read {}: {}", args.target, e))?;
            let (debug_caps, caps): (Vec<&str>, Vec<&str>) = args
                .capabilities
                .iter()
                .map(|s| s.as_str())
                .partition(|c| c.starts_with("debug."));
            let visibility = debug_visibility(&debug_caps)?;
            let permissions: Option<&[&str]> =
                if caps.is_empty() { None } else { Some(&caps) };
            let mut program = crush_vm::assemble(&source, permissions, Some(&args.target))
                .map_err(|e| anyhow::anyhow!("assemble failed: {}", e))?;
            let source_map = std::mem::take(&mut program.source_map);
            let mut vm = crush_vm::PortableVm::new(program);
            let mut grants = crush_vm::HostCaps::new();
            grants.grant_debug(visibility);
            vm.set_host_caps(grants);
            {
                let mut quotas = crush_vm::Quotas::default();
                let mut set = false;
                if let Some(v) = args.max_steps {
                    quotas.max_steps = v;
                    set = true;
                }
                if let Some(v) = args.max_stack {
                    quotas.max_stack = v;
                    set = true;
                }
                if let Some(v) = args.max_output {
                    quotas.max_output = v;
                    set = true;
                }
                if let Some(v) = args.max_call_depth {
                    quotas.max_call_depth = v;
                    set = true;
                }
                if set {
                    vm.set_quotas(quotas);
                }
            }
            let driver = PortableVmDriver::new(&mut vm);
            let mut session = DebugSession::new(driver, source_map);
            for arg in &args.breakpoints {
                let (file, line) = repl::parse_breakpoint_arg(arg)
                    .map_err(|e| anyhow::anyhow!("bad --break `{}`: {}", arg, e))?;
                let id = session.add_breakpoint(file, line);
                eprintln!("breakpoint #{} set at {}", id.0, arg);
            }
            session.run_repl()?;
            Ok(())
        }
        Cmd::Repl => {
            eprintln!(
                "crush-debugger: `repl` subcommand is not yet implemented. \
                 Use `run <file.crush>` to start the debugger."
            );
            Ok(())
        }
    }
}
