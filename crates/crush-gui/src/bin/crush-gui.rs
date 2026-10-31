//! `crush-gui` — run a Crush program that draws a window through `ui.*`.
//!
//! ```text
//! crush-gui run app.crush --ui
//! crush gui run app.crush --ui        # the umbrella CLI delegates here
//! ```
//!
//! `--ui` is the grant for the `ui.*` capabilities; without it a program that
//! names one is refused before it runs. The other grants are the ones
//! `crush-run` has (`--fs`, `--net`, …) and mean the same thing here.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use crush_gui::{Grants, NotGranted, RunEnd, Source, desktop, execute, link};

#[derive(Parser)]
#[command(
    name = "crush-gui",
    version,
    about = "Run Crush programs that draw a window with ui.*"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a .crush program (or .cvm1 blob) in a desktop window.
    Run(RunArgs),
}

#[derive(Parser)]
struct RunArgs {
    /// Path to the program (.crush, .cvm1 or .cvm).
    path: PathBuf,

    /// Arguments for the program, returned by `sys.args()`.
    #[arg(value_name = "ARGS")]
    script_args: Vec<String>,

    /// Grant the `ui.*` capabilities: render a window, read its events.
    #[arg(long)]
    ui: bool,

    /// Enable filesystem host capabilities, confined to --fs-root.
    #[arg(long)]
    fs: bool,

    /// Sandbox filesystem access under this directory.
    #[arg(long, value_name = "DIR", default_value = ".")]
    fs_root: PathBuf,

    /// Enable environment-variable host capabilities.
    #[arg(long)]
    env: bool,

    /// Enable time host capabilities (time.now, time.sleep, …).
    #[arg(long)]
    time: bool,

    /// Enable message-bus capabilities.
    #[arg(long)]
    bus: bool,

    /// Enable task-management capabilities.
    #[arg(long)]
    task: bool,

    /// Enable knowledge-graph capabilities.
    #[arg(long)]
    akg: bool,

    /// Enable `process.exec`.
    #[arg(long)]
    process: bool,

    /// Enable cryptography host capabilities.
    #[arg(long)]
    crypto: bool,

    /// Enable network host capabilities (net.http_*).
    #[arg(long)]
    net: bool,

    /// Enable polyglot blocks (@python / @javascript / @bash).
    #[arg(long)]
    polyglot: bool,

    /// Maximum instruction steps (default: 500,000,000 - a UI loop is long-lived).
    #[arg(long, value_name = "N")]
    max_steps: Option<usize>,

    /// Maximum output bytes.
    #[arg(long, value_name = "N")]
    max_output: Option<usize>,
}

impl RunArgs {
    fn grants(&self) -> Grants {
        Grants {
            ui: self.ui,
            fs: self.fs,
            fs_root: self.fs_root.clone(),
            env: self.env,
            time: self.time,
            bus: self.bus,
            task: self.task,
            akg: self.akg,
            process: self.process,
            crypto: self.crypto,
            net: self.net,
            polyglot: self.polyglot,
            args: self.script_args.clone(),
            max_steps: self.max_steps,
            max_output: self.max_output,
        }
    }
}

fn main() -> ExitCode {
    let Commands::Run(args) = Cli::parse().command;
    crush_lang_sdk::theme::init_styling();
    run(args)
}

fn run(args: RunArgs) -> ExitCode {
    if !crush_gui::run::is_program(&args.path) {
        eprintln!(
            "crush-gui: {} is not a .crush / .cvm1 program",
            args.path.display()
        );
        return ExitCode::from(2);
    }
    let grants = args.grants();
    let source = Source::File(args.path.clone());

    // The program runs on its own thread, behind a channel pair; this (the
    // main) thread owns the window, because winit insists on it.
    let (program_end, surface) = link();
    let program = std::thread::spawn(move || {
        let mut out = |part: &str| {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(part.as_bytes());
            let _ = stdout.flush();
        };
        match execute(&source, &grants, Arc::new(program_end), &mut out) {
            // Ran to the end: if it drew anything the window stays up, showing
            // the last page, until it is closed.
            Ok(RunEnd::Finished) => {}
            // `sys.exit` and errors end the process, window or not.
            Ok(RunEnd::Exit(code)) => std::process::exit(code),
            Err(e) => {
                let label = if e.is::<NotGranted>() {
                    "capabilities"
                } else {
                    "runtime"
                };
                eprint!("{}", crush_lang_sdk::theme::render_anyhow_error(&e, label));
                std::process::exit(1);
            }
        }
    });

    // Returns once the window is closed - or at once, without opening one,
    // when the program ends before ever rendering.
    let window = desktop::run_window(surface);
    // The program has been sent `close`; let it finish what it does after.
    let _ = program.join();
    match window {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("crush-gui: {e}");
            ExitCode::FAILURE
        }
    }
}
