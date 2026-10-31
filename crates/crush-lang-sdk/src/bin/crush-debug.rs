//! `crush-debug` — interactive debugger for Crush programs (CRUSH-317).
//!
//! `crush debug FILE` runs this. It compiles a `.crush` file through the same
//! pipeline as `crush run` — keeping the source map, so `break FILE:LINE`
//! resolves, and the local names, so `print total` works — and drops into
//! `crush-debugger`'s REPL. A `.casm` file is assembled instead (lines are the
//! assembly's, and locals are slots).
//!
//! Debugging is a capability like any other, and nothing is ambient: without
//! `--step` the debugger can only `list`/`status`/`help`; `--inspect=redacted`
//! lets `print` show types and hashes; `--inspect` shows values. The program's
//! own capabilities are granted with the flags `crush run` takes.
//!
//! ```bash
//! printf 'break 12\ncontinue\nprint x\nfinish\ncontinue\n' | crush-debug prog.crush --step --inspect
//! crush-debug prog.crush --step --inspect --message-format json < commands.txt
//! ```
//!
//! Commands are read from stdin, so the program's own `io.read` shares it.
//! Never leave stdin open and unread.

use std::io::{BufReader, Cursor, Read};
use std::path::PathBuf;

use clap::Parser;
use crush_debugger::{DebugSession, PortableVmDriver, repl};
use crush_lang_sdk::{HostCapsBuilder, MessageFormat, Runtime};
use crush_vm::{DebugVisibility, Quotas};

#[derive(Parser)]
#[command(
    name = "crush-debug",
    version,
    about = "Debug a Crush program: breakpoints, step/next/finish, watch, print",
    long_about = "Debug a Crush program: breakpoints, step/next/finish, watch, print.\n\n\
        Debugging is grant-gated and grants nothing by default: pass --step to run, break \
        and step, and --inspect (or --inspect=redacted) to see values. Commands are read \
        from stdin (try `help`); the program's own capabilities are granted with the same \
        flags as `crush run`."
)]
struct Args {
    /// Program to debug (.crush, or .casm assembly).
    path: PathBuf,

    /// Arguments for the program, returned by `sys.args()`.
    #[arg(value_name = "ARGS")]
    script_args: Vec<String>,

    /// Grant `debug.step`: break, step, next, finish, continue and watch. Without it
    /// the debugger refuses every command that runs the program.
    #[arg(long)]
    step: bool,

    /// Grant value inspection (needs --step): `--inspect` shows values in full
    /// (`debug.inspect`); `--inspect=redacted` shows type and a per-session hash
    /// (`debug.inspect.redacted`). Without either, values show as <hidden>.
    #[arg(long, value_name = "LEVEL", num_args = 0..=1, require_equals = true,
          default_missing_value = "full", value_parser = ["full", "redacted"])]
    inspect: Option<String>,

    /// Set a breakpoint before the first command (repeatable): `FILE:LINE` or `LINE`.
    /// Needs --step.
    #[arg(long = "break", value_name = "FILE:LINE")]
    breakpoints: Vec<String>,

    /// `text` (default): a prompt and prose. `json`: no prompt; every event the
    /// debugger reports is one NDJSON object on stdout (see crush-debugger's
    /// `ndjson` module), for an editor or agent driving it over a pipe.
    #[arg(long = "message-format", value_name = "FORMAT", default_value = "text")]
    message_format: MessageFormat,

    // ---- the program's own capabilities: the same flags as `crush run` ----
    /// Grant a capability permission to the program (repeatable).
    #[arg(long = "cap", value_name = "CAP")]
    caps: Vec<String>,

    /// Enable filesystem host capabilities, confined to --fs-root.
    #[arg(long)]
    fs: bool,

    /// Enable polyglot execution (@python / @javascript / @bash blocks).
    #[arg(long)]
    polyglot: bool,

    /// Sandbox filesystem access under this directory.
    #[arg(long, value_name = "DIR", default_value = ".")]
    fs_root: PathBuf,

    /// Enable environment-variable host capabilities.
    #[arg(long)]
    env: bool,

    /// Enable time host capabilities.
    #[arg(long)]
    time: bool,

    /// Enable message-bus capabilities.
    #[arg(long)]
    bus: bool,

    /// Enable task-management capabilities (also grants process/spawn).
    #[arg(long)]
    task: bool,

    /// Enable knowledge-graph capabilities.
    #[arg(long)]
    akg: bool,

    /// Enable process host capability (process.exec).
    #[arg(long)]
    process: bool,

    /// Enable cryptography host capabilities.
    #[arg(long)]
    crypto: bool,

    /// Enable graphics host capabilities.
    #[arg(long)]
    graphics: bool,

    /// Enable network host capabilities.
    #[arg(long)]
    net: bool,

    /// Maximum HTTP response size in bytes.
    #[arg(long, value_name = "N", default_value = "1048576")]
    net_max_response_bytes: usize,

    /// Enable database host capabilities on this path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    /// Do not register the standard library capabilities.
    #[arg(long)]
    no_stdlib: bool,

    /// Maximum instruction steps.
    #[arg(long, value_name = "N")]
    max_steps: Option<usize>,

    /// Maximum stack slots.
    #[arg(long, value_name = "N")]
    max_stack: Option<usize>,

    /// Maximum output bytes.
    #[arg(long, value_name = "N")]
    max_output: Option<usize>,

    /// Maximum call depth.
    #[arg(long, value_name = "N")]
    max_call_depth: Option<usize>,
}

/// A refused command names the flag that grants it.
fn grant_flag(grant: &str) -> String {
    match grant {
        "debug.step" => "pass --step".to_string(),
        "debug.inspect.redacted" => "pass --step --inspect=redacted, or --inspect".to_string(),
        other => format!("pass --step --inspect (grant `{other}`)"),
    }
}

impl Args {
    fn visibility(&self) -> anyhow::Result<DebugVisibility> {
        Ok(match (self.step, self.inspect.as_deref()) {
            (false, None) => DebugVisibility::None,
            (false, Some(_)) => anyhow::bail!(
                "--inspect lets the debugger show values, but only while it may run the \
                 program: add --step"
            ),
            (true, None) => DebugVisibility::ControlOnly,
            (true, Some("redacted")) => DebugVisibility::InspectRedacted,
            (true, Some(_)) => DebugVisibility::Full,
        })
    }

    fn quotas(&self) -> Quotas {
        let mut quotas = Quotas::default();
        if let Some(n) = self.max_steps {
            quotas.max_steps = n;
        }
        if let Some(n) = self.max_stack {
            quotas.max_stack = n;
        }
        if let Some(n) = self.max_output {
            quotas.max_output = n;
        }
        if let Some(n) = self.max_call_depth {
            quotas.max_call_depth = n;
        }
        quotas
    }

    /// The program's host capabilities, granted like `crush run` grants them.
    fn host_caps(&self) -> anyhow::Result<crush_vm::HostCaps> {
        #[allow(unused_mut)]
        let mut builder = HostCapsBuilder::new()
            .polyglot(if self.polyglot {
                &["python", "javascript", "bash"]
            } else {
                &[]
            })
            .fs(self.fs)
            .fs_root(self.fs_root.to_string_lossy())
            .env(self.env)
            .time(self.time)
            .bus(self.bus)
            .task(self.task)
            .akg(self.akg)
            .process(self.process)
            .crypto(self.crypto);
        #[cfg(feature = "graphics")]
        {
            builder = builder.graphics(self.graphics);
        }
        #[cfg(not(feature = "graphics"))]
        if self.graphics {
            eprintln!(
                "warning: --graphics requires the 'graphics' feature (not enabled in this build)"
            );
        }
        #[cfg(feature = "net")]
        {
            builder = builder
                .net(self.net)
                .net_max_response_bytes(self.net_max_response_bytes);
        }
        #[cfg(not(feature = "net"))]
        if self.net {
            eprintln!("warning: --net requires the 'net' feature (not enabled in this build)");
        }
        #[cfg(feature = "db")]
        if let Some(path) = &self.db {
            builder = builder.db(path.to_string_lossy());
        }
        #[cfg(not(feature = "db"))]
        if self.db.is_some() {
            eprintln!("warning: --db requires the 'db' feature (not enabled in this build)");
        }
        #[cfg(feature = "stdlib")]
        {
            builder = builder.stdlib(!self.no_stdlib);
        }
        Ok(builder.args(self.script_args.clone()).build())
    }
}

/// The program to debug, its source map (in the file's own lines) and local names.
fn load(args: &Args) -> anyhow::Result<(crush_vm::Program, crush_lang_sdk::compile::LocalNames)> {
    let ext = args.path.extension().and_then(|s| s.to_str()).unwrap_or("");
    match ext {
        "crush" => {
            let source = std::fs::read_to_string(&args.path)?;
            crush_lang_sdk::compile::compile_crush_source_for_debug(&source)
                .map_err(|e| anyhow::Error::new(CompileFailed(e)))
        }
        "casm" => {
            let source = std::fs::read_to_string(&args.path)?;
            let mut permissions: Vec<&str> = args.caps.iter().map(String::as_str).collect();
            if args.polyglot {
                permissions.extend(["polyglot.python", "polyglot.javascript", "polyglot.bash"]);
            }
            let program = crush_lang_sdk::assemble(&source, Some(&permissions), None)?;
            Ok((program, Default::default()))
        }
        _ => anyhow::bail!(
            "unsupported file extension: {ext} (crush-debug takes .crush source or .casm \
             assembly; a compiled .cvm1 has no source map to break on)"
        ),
    }
}

#[derive(Debug)]
struct CompileFailed(anyhow::Error);

impl std::fmt::Display for CompileFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for CompileFailed {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

fn run(args: &Args) -> anyhow::Result<()> {
    let visibility = args.visibility()?;
    if !args.breakpoints.is_empty() && !args.step {
        anyhow::bail!("--break sets breakpoints, which needs --step");
    }
    let mut commands = String::new();
    for arg in &args.breakpoints {
        repl::parse_breakpoint_arg(arg).map_err(|e| anyhow::anyhow!("bad --break `{arg}`: {e}"))?;
        commands.push_str(&format!("break {arg}\n"));
    }

    let is_crush = args.path.extension().and_then(|s| s.to_str()) == Some("crush");
    let (mut program, local_names) = load(args)?;
    for warning in &program.warnings {
        eprintln!("warning: {warning}");
    }
    let source_map = std::mem::take(&mut program.source_map);

    // Refuse before anything runs when the program can request a capability
    // this run doesn't grant, as `crush run` does (CRUSH-243).
    let preflight = Runtime::with_quotas(args.quotas()).with_host_caps(args.host_caps()?);
    let missing = preflight.missing_grants(&program)?;
    if !missing.is_empty() {
        anyhow::bail!(
            "{} needs capabilities this run does not grant; nothing was run:\n{}",
            args.path.display(),
            crush_lang_sdk::effects::describe_missing(&missing)
        );
    }

    let mut caps = args.host_caps()?;
    caps.grant_debug(visibility);
    let mut vm = crush_vm::PortableVm::new(program);
    vm.set_host_caps(caps);
    vm.set_quotas(args.quotas());

    let mut session = DebugSession::new(PortableVmDriver::new(&mut vm), source_map);
    session.set_source_name(args.path.display().to_string());
    session.set_local_names(local_names);
    if let Ok(text) = std::fs::read_to_string(&args.path) {
        session.set_source_text(&text);
    }
    // `.crush` has Crush lines in its source map, so `step`/`next` go by line;
    // `.casm` keeps instruction-level stepping.
    session.set_line_stepping(is_crush);
    session.set_grant_hint(grant_flag);

    let stdin = std::io::stdin();
    let input = BufReader::new(Cursor::new(commands).chain(stdin.lock()));
    match args.message_format {
        MessageFormat::Text => session.run_repl_io(input, std::io::stdout()),
        MessageFormat::Json => session.run_ndjson(input, std::io::stdout()),
    }
}

fn main() {
    let args = Args::parse();
    crush_lang_sdk::theme::init_styling();
    if let Err(e) = run(&args) {
        match args.message_format {
            MessageFormat::Text => {
                let label = if e.is::<CompileFailed>() {
                    "compile"
                } else {
                    "debug"
                };
                eprint!("{}", crush_lang_sdk::theme::render_anyhow_error(&e, label));
            }
            MessageFormat::Json => {
                let diag = crush_lang_sdk::theme::JsonDiagnostic::generic_error(
                    &e.to_string(),
                    crush_lang_sdk::theme::JsonDiagnostic::CODE_IO,
                );
                eprintln!("{}", diag.to_line());
            }
        }
        std::process::exit(1);
    }
}
