//! `crush-run` — CLI runner for CRUSH/CVM1 programs.
//!
//! Run a CASM text file or a compiled CVM1 binary with optional host
//! capabilities and resource limits.
//!
//! # Examples
//!
//! ```bash
//! # Assemble and run CASM text
//! crush-run run hello.casm
//!
//! # Run a compiled CVM1 blob
//! crush-run run program.cvm1
//!
//! # Enable filesystem + env + time host caps
//! crush-run run script.casm --fs --env --time --cap fs.read --cap env.get --cap time.now
//!
//! # Restrict execution
//! crush-run run script.casm --max-steps 100000 --max-stack 1024
//! ```

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use crush_lang_sdk::{HostCapsBuilder, Runtime};
use crush_vm::{Quotas, VmResult};

#[derive(Parser)]
#[command(name = "crush-run")]
#[command(about = "Run CRUSH/CVM1 programs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a CASM text file or CVM1 binary.
    Run(RunArgs),

    /// List built-in portable capabilities.
    Caps,
}

#[derive(Parser)]
struct RunArgs {
    /// Path to the program file (.casm or .cvm1).
    path: PathBuf,

    /// Grant a capability permission (repeatable).
    #[arg(long = "cap", value_name = "CAP")]
    caps: Vec<String>,

    /// Enable filesystem host capabilities (fs.read, fs.write, fs.exists, fs.list).
    #[arg(long)]
    fs: bool,

    /// Sandbox filesystem access under this directory.
    #[arg(long, value_name = "DIR", default_value = ".")]
    fs_root: PathBuf,

    /// Enable environment-variable host capability (env.get).
    #[arg(long)]
    env: bool,

    /// Enable time host capability (time.now).
    #[arg(long)]
    time: bool,

    /// Enable message-bus capabilities (message_bus.publish/subscribe/recv).
    #[arg(long)]
    bus: bool,

    /// Enable task-management capabilities (task.start/stop/list).
    #[arg(long)]
    task: bool,

    /// Enable knowledge-graph capabilities (akg.write/read/search).
    #[arg(long)]
    akg: bool,

    /// Enable process host capability (process.exec).
    #[arg(long)]
    process: bool,

    /// Enable cryptography host capabilities (crypto.sha256, crypto.random).
    #[arg(long)]
    crypto: bool,

    /// Enable graphics host capabilities (graphics.canvas/rect/circle/text/to_svg).
    #[arg(long)]
    graphics: bool,

    /// Enable standard library capabilities (str.*, math.*, conv.*, collections.*, json.*, path.*, regex.*).
    #[arg(long)]
    stdlib: bool,

    /// Enable network host capabilities (net.http_get, net.http_post).
    #[arg(long)]
    net: bool,

    /// Maximum HTTP response size in bytes.
    #[arg(long, value_name = "N", default_value = "1048576")]
    net_max_response_bytes: usize,

    /// Enable database host capabilities (db.query, db.execute) on this path.
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

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

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Caps => list_caps(),
        Commands::Run(args) => {
            if let Err(e) = run_file(&args) {
                eprintln!("crush-run: {e:#}");
                std::process::exit(1);
            }
        }
    }
}

fn list_caps() {
    println!("Built-in portable capabilities:");
    println!("  io.print      write args to stdout");
    println!("  str.concat    concatenate args → string");
    println!("  str.len       byte length of a string");
    println!();
    println!("Host capabilities (enable with --fs / --env / --time):");
    println!("  fs.read PATH           read file contents");
    println!("  fs.write PATH DATA     write file contents");
    println!("  fs.exists PATH         return 1 if file exists, else 0");
    println!("  fs.list DIR            list directory entries");
    println!("  env.get NAME           read environment variable");
    println!("  time.now               return Unix timestamp");
    println!("Message-bus capabilities (enable with --bus):");
    println!("  message_bus.publish TOPIC PAYLOAD");
    println!("  message_bus.subscribe TOPIC");
    println!("  message_bus.recv       blocking receive");
    println!("Task capabilities (enable with --task):");
    println!("  task.start NAME COMMAND [ARGS...]");
    println!("  task.stop TASK_ID");
    println!("  task.list");
    println!("Knowledge-graph capabilities (enable with --akg):");
    println!("  akg.write ID JSON_UNIT");
    println!("  akg.read ID");
    println!("  akg.search QUERY");
    println!("Process capabilities (enable with --process):");
    println!("  process.exec CMD [ARGS...]  run subprocess, return JSON stdout/stderr/exit_code");
    println!("Cryptography capabilities (enable with --crypto):");
    println!("  crypto.sha256 DATA          return hex SHA-256 digest");
    println!("  crypto.random N             return N random bytes as base64 (max 4096)");
    println!("Graphics capabilities (enable with --graphics):");
    println!("  graphics.canvas W H         create canvas, return handle");
    println!("  graphics.rect HANDLE X Y W H FILL");
    println!("  graphics.circle HANDLE CX CY R FILL");
    println!("  graphics.text HANDLE X Y CONTENT FILL");
    println!("  graphics.to_svg HANDLE      return SVG XML");
    #[cfg(feature = "net")]
    {
        println!("Network capabilities (enable with --net):");
        println!("  net.http_get URL       HTTP GET request");
        println!("  net.http_post URL BODY HTTP POST request");
    }
    #[cfg(feature = "db")]
    {
        println!("Database capabilities (enable with --db PATH):");
        println!("  db.query SQL [PARAMS...]  execute SELECT, return rows");
        println!("  db.execute SQL [PARAMS...] execute INSERT/UPDATE/DELETE, return affected rows");
    }
    #[cfg(feature = "stdlib")]
    {
        println!("Standard library capabilities (enable with --stdlib):");
        println!(
            "  str.len/split/join/trim/replace/contains/starts_with/ends_with/to_upper/to_lower"
        );
        println!("  str.pad_left/pad_right/repeat/substring/char_at/index_of/format");
        println!("  math.sqrt/abs/floor/ceil/round/sin/cos/tan/pow/min/max/pi");
        println!("  conv.to_int/to_float/to_str/to_bool/parse_int/parse_float/type_of");
        println!("  collections.len/reverse/includes/flatten/chunk/zip/unique");
        println!("  json.parse/stringify/stringify_pretty");
        println!("  path.join/dirname/basename/extension/is_absolute/normalize/stem");
        println!("  regex.test/match/find_all/replace/split");
    }
}

fn run_file(args: &RunArgs) -> anyhow::Result<()> {
    let ext = args.path.extension().and_then(|s| s.to_str()).unwrap_or("");

    let program = match ext {
        "crush" => {
            let source = std::fs::read_to_string(&args.path)?;
            crush_lang_sdk::compile::compile_crush_source(&source)?
        }
        "casm" => {
            let source = std::fs::read_to_string(&args.path)?;
            let permissions: Vec<&str> = args.caps.iter().map(|s| s.as_str()).collect();
            crush_lang_sdk::assemble(&source, Some(&permissions), None)?
        }
        "cvm1" => {
            let blob = std::fs::read(&args.path)?;
            crush_vm::Program::from_blob(&blob)?
        }
        _ => anyhow::bail!(
            "unsupported file extension: {} (expected .crush, .casm, or .cvm1)",
            ext
        ),
    };

    let mut quotas = Quotas::default();
    if let Some(n) = args.max_steps {
        quotas.max_steps = n;
    }
    if let Some(n) = args.max_stack {
        quotas.max_stack = n;
    }
    if let Some(n) = args.max_output {
        quotas.max_output = n;
    }
    if let Some(n) = args.max_call_depth {
        quotas.max_call_depth = n;
    }

    #[allow(unused_mut)]
    let mut builder = HostCapsBuilder::new()
        .fs(args.fs)
        .fs_root(args.fs_root.to_string_lossy())
        .env(args.env)
        .time(args.time)
        .bus(args.bus)
        .task(args.task)
        .akg(args.akg)
        .process(args.process)
        .crypto(args.crypto);

    #[cfg(feature = "graphics")]
    {
        builder = builder.graphics(args.graphics);
    }
    #[cfg(not(feature = "graphics"))]
    if args.graphics {
        eprintln!(
            "warning: --graphics requires the 'graphics' feature (not enabled in this build)"
        );
    }

    #[cfg(feature = "net")]
    {
        builder = builder
            .net(args.net)
            .net_max_response_bytes(args.net_max_response_bytes);
    }
    #[cfg(not(feature = "net"))]
    if args.net {
        eprintln!("warning: --net requires the 'net' feature (not enabled in this build)");
    }

    #[cfg(feature = "db")]
    if let Some(ref db_path) = args.db {
        builder = builder.db(db_path.to_string_lossy());
    }
    #[cfg(not(feature = "db"))]
    if args.db.is_some() {
        eprintln!("warning: --db requires the 'db' feature (not enabled in this build)");
    }

    #[cfg(feature = "stdlib")]
    if args.stdlib {
        builder = builder.stdlib(true);
    }
    #[cfg(not(feature = "stdlib"))]
    if args.stdlib {
        eprintln!("warning: --stdlib requires the 'stdlib' feature (not enabled in this build)");
    }

    let host_caps = builder.build();

    let runtime = Runtime::with_quotas(quotas).with_host_caps(host_caps);

    let result = runtime.run(&program)?;
    print_result(&result);
    if !result.halted {
        eprintln!("(program fell off end without HALT)");
    }
    Ok(())
}

fn print_result(result: &VmResult) {
    print!("{}", result.output);
    eprintln!("[steps={}, stack={}]", result.steps, result.stack.len());
}
