//! `crush-run` — CLI runner for CRUSH/CVM1 programs.
//!
//! Run a Crush program in any of its forms — source, CAST, CASM, CVM1
//! assembly or CVM1 bytecode (`docs/FILE-FORMATS.md`, #365) — with optional
//! host capabilities and resource limits.
//!
//! # Examples
//!
//! ```bash
//! crush-run run hello.crush          # source
//! crush-run run hello.cast.json      # CAST (.cast.json / .cast / .castb)
//! crush-run run hello.casm           # CASM JSON (.casm / .casmb / .sno)
//! crush-run run hello.cvms           # CVM1 assembly (its .cap lines declare caps)
//! crush-run run program.cvm1         # CVM1 bytecode (.cvm1 / .cvm)
//!
//! # Enable filesystem + env + time host caps
//! crush-run run script.cvms --fs --env --time --cap fs.read --cap env.get --cap time.now
//!
//! # Restrict execution
//! crush-run run script.cvms --max-steps 100000 --max-stack 1024
//! ```

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use crush_lang_sdk::theme::{JsonDiagnostic, JsonDiagnosticExt as _};
use crush_lang_sdk::{HostCapsBuilder, MessageFormat, Runtime};
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
    /// Run a program: .crush, .cast.json/.cast/.castb, .casm/.casmb/.sno, .cvms or .cvm1/.cvm.
    Run(RunArgs),

    /// List built-in portable capabilities, or, given a program, the
    /// capabilities it can use and what grants each (without running it).
    Caps {
        /// Print as JSON instead of the human-readable list.
        #[arg(long)]
        json: bool,

        /// A program (any format `run` takes) to inspect instead of listing
        /// every capability.
        path: Option<PathBuf>,

        /// Add a directory to the import search path (repeatable; see `run -L`).
        #[arg(short = 'L', long = "lib-path", value_name = "DIR")]
        lib_paths: Vec<PathBuf>,
    },
}

#[derive(Parser)]
struct RunArgs {
    /// Path to the program: .crush source, CAST (.cast.json, .cast, .castb), CASM JSON
    /// (.casm, .sno; .casmb MessagePack), CVM1 assembly (.cvms) or CVM1 bytecode (.cvm1, .cvm).
    path: PathBuf,

    /// Arguments for the program, returned by `sys.args()`. Flags for
    /// crush-run may come before or after them; put `--` before an argument
    /// that starts with `-`.
    #[arg(value_name = "ARGS")]
    script_args: Vec<String>,

    /// Grant a capability permission (repeatable).
    #[arg(long = "cap", value_name = "CAP")]
    caps: Vec<String>,

    /// Add a directory to the import search path (repeatable). `import a.b`
    /// looks for `a/b.crush` in the program's directory, then each `-L`
    /// directory in order, then each `CRUSH_PATH` directory (CRUSH-110).
    #[arg(short = 'L', long = "lib-path", value_name = "DIR")]
    lib_paths: Vec<PathBuf>,

    /// Enable filesystem host capabilities (fs.read/write/exists/list, fs.ls/cat/pwd/cd/mkdir/rm/cp/mv/touch/find, text.head/tail/wc/cut/grep), confined to --fs-root.
    #[arg(long)]
    fs: bool,

    /// Enable polyglot execution (@python / @javascript / @bash blocks spawn interpreters with
    /// the host process's authority). OFF by default — polyglot is NOT ambient.
    #[arg(long)]
    polyglot: bool,

    /// Sandbox filesystem access under this directory.
    #[arg(long, value_name = "DIR", default_value = ".")]
    fs_root: PathBuf,

    /// Enable environment-variable host capabilities (env.get, env.all, env.home_dir).
    #[arg(long)]
    env: bool,

    /// Enable time host capabilities (time.now, time.now_ms, time.now_iso, time.elapsed, time.sleep, async.sleep).
    #[arg(long)]
    time: bool,

    /// Enable message-bus capabilities (message_bus.publish/subscribe/recv).
    #[arg(long)]
    bus: bool,

    /// Enable task-management capabilities (task.start/stop/list). `task.start`
    /// spawns a host process, so `--task` also grants `process/spawn`.
    #[arg(long)]
    task: bool,

    /// Enable knowledge-graph capabilities (akg.write/read/search).
    #[arg(long)]
    akg: bool,

    /// Index these `.crush` files (a directory expands to every `*.crush` below
    /// it; repeatable) and enable the read-only `codebase.*` capabilities over
    /// them: `codebase.modules()`, `.definition(name)`, `.callers(name)`,
    /// `.invariants(name)`, `.exhaustive_sites(name)`, `.wip()`, `.temporaries()`,
    /// `.stale_temporaries()`, `.decisions()`, ... Passing the flag is the grant:
    /// without it the program cannot see any source but its own.
    #[arg(long = "codebase", value_name = "FILE_OR_DIR")]
    codebase: Vec<PathBuf>,

    /// Enable process host capability (process.exec).
    #[arg(long)]
    process: bool,

    /// Enable cryptography host capabilities (crypto.sha256, crypto.random).
    #[arg(long)]
    crypto: bool,

    /// Enable `gpu.launch`: run the program's GPU kernels on an NVIDIA GPU
    /// (CRUSH-zorro-3). Needs a build with the `gpu` feature.
    #[arg(long)]
    gpu: bool,

    /// Enable graphics host capabilities (graphics.canvas/rect/circle/text/to_svg).
    #[arg(long)]
    graphics: bool,

    /// Standard library capabilities (str.*, math.*, conv.*, collections.*, json.*, path.*, regex.*, bytes.*, buffer.*, binary.*, result.*, text.sort/uniq, time.format/parse, env.os/arch, system.* SBL) are on by default; this flag is kept for compatibility. It is an error in a build without the `stdlib` feature.
    #[arg(long, conflicts_with = "no_stdlib")]
    stdlib: bool,

    /// Do not register the standard library capabilities.
    #[arg(long)]
    no_stdlib: bool,

    /// Enable network host capabilities (net.http_get/post/put/delete, net.http_request).
    #[arg(long)]
    net: bool,

    /// Maximum HTTP response size in bytes.
    #[arg(long, value_name = "N", default_value = "1048576")]
    net_max_response_bytes: usize,

    /// Enable raw TCP client sockets (tcp.connect/send/recv/close/ping/tls_wrap). Separate from
    /// --net (HTTP only); does not allow tcp.listen/accept.
    #[arg(long)]
    tcp: bool,

    /// Enable raw TCP SERVER sockets (tcp.listen/accept, plus send/recv/close/ping on the
    /// connections they return): the program becomes reachable from other hosts. Does not
    /// allow outbound tcp.connect; add --tcp for that.
    #[arg(long = "tcp-listen")]
    tcp_listen: bool,

    /// Enable database host capabilities (db.query, db.execute) on this path.
    /// Opt-in at build time: needs a build with `--features db`.
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

    /// Format for diagnostic output on errors: `text` (default, themed
    /// terminal output with cause-chain walk) or `json` (newline-delimited
    /// records for editor / IDE / LSP bridge integration).
    #[arg(long = "message-format", value_name = "FORMAT", default_value = "text")]
    message_format: MessageFormat,
}

fn main() {
    let cli = Cli::parse();
    crush_lang_sdk::theme::init_styling();
    match cli.command {
        Commands::Caps { json, path: Some(path), lib_paths } => {
            if let Err(e) = show_program_caps(&path, json, &lib_paths) {
                eprint!("{}", crush_lang_sdk::theme::render_anyhow_error(&e, "compile"));
                std::process::exit(1);
            }
        }
        Commands::Caps { json: false, path: None, .. } => list_caps(),
        Commands::Caps { json: true, path: None, .. } => list_caps_json(),
        Commands::Run(args) => {
            if let Err(e) = run_file(&args) {
                match args.message_format {
                    MessageFormat::Text => {
                        // Walk anyhow's full cause chain — works for any
                        // error type produced by `run_file`.
                        let label = if e.is::<CompileFailed>() {
                            "compile"
                        } else if e.is::<NotGranted>() {
                            "capabilities"
                        } else {
                            "runtime"
                        };
                        eprint!("{}", crush_lang_sdk::theme::render_anyhow_error(&e, label));
                    }
                    MessageFormat::Json => {
                        // The same records `crushc` emits for the same input
                        // (CRUSH-315): syntax errors -> E-PP0n with position,
                        // semantic errors -> E-TP01 with position, a run that
                        // fails -> E-RT0n with the error text in `message`, a
                        // refused capability -> E-CAPS. The program's path is
                        // attached to every record.
                        let file = args.path.display().to_string();
                        let diags = if let Some(refused) = e.downcast_ref::<NotGranted>() {
                            vec![
                                JsonDiagnostic::error(JsonDiagnostic::CODE_CAPS, &refused.message)
                                    .with_hint(&refused.hint)
                                    .in_file(Some(&file)),
                            ]
                        } else {
                            let cause = e.downcast_ref::<CompileFailed>().map_or(&e, |c| &c.0);
                            crush_lang_sdk::theme::diagnostics_for_error(
                                cause,
                                Some(&file),
                                JsonDiagnostic::CODE_IO,
                            )
                        };
                        eprint!(
                            "{}",
                            crush_lang_sdk::theme::render_diagnostics_ndjson(&diags)
                        );
                    }
                }
                std::process::exit(1);
            }
        }
    }
}

/// The program can request capabilities this run doesn't grant, found before
/// it ran (CRUSH-243), so `main` labels it `[capabilities]`.
#[derive(Debug)]
struct NotGranted {
    /// The full text-mode explanation (what `Display` prints).
    text: String,
    /// One line for a `message`: what is wrong.
    message: String,
    /// What to do about it: the flag that grants each missing capability.
    hint: String,
}

impl NotGranted {
    fn new(path: &std::path::Path, missing: &[crush_lang_sdk::effects::MissingGrant]) -> Self {
        let caps: Vec<&str> = missing.iter().map(|m| m.capability.as_str()).collect();
        let mut flags: Vec<&str> = missing.iter().filter_map(|m| m.grant).collect();
        flags.sort_unstable();
        flags.dedup();
        let hint = if flags.is_empty() {
            "no grant in this build provides them".to_string()
        } else {
            format!("grant with {}", flags.join(" "))
        };
        Self {
            text: format!(
                "{} needs capabilities this run does not grant; nothing was run:\n{}",
                path.display(),
                crush_lang_sdk::effects::describe_missing(missing)
            ),
            message: format!(
                "needs capabilities this run does not grant; nothing was run: {}",
                caps.join(", ")
            ),
            hint,
        }
    }
}

impl std::fmt::Display for NotGranted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::error::Error for NotGranted {}

/// Compile `path` to bytecode, plus the `@capabilities` it declares
/// (`.crush` and CAST only).
fn load_program(
    path: &std::path::Path,
    casm_caps: &[String],
    polyglot: bool,
    lib_paths: &[PathBuf],
    notes: bool,
) -> anyhow::Result<(crush_vm::Program, Option<Vec<String>>)> {
    use crush_lang_sdk::formats::{self, InputFormat};
    let format = formats::detect_or_bail(path, notes)?;
    match format {
        InputFormat::Crush => {
            let source = std::fs::read_to_string(path)?;
            let declared = crush_frontend::parse_source(&source)
                .ok()
                .and_then(|p| p.manifest)
                .and_then(|m| m.capabilities);
            let imports = import_options(lib_paths);
            let program = crush_lang_sdk::compile::compile_crush_file(
                path,
                &imports,
                &crush_lang_sdk::compile::CompileOptions::default(),
            )
            .map_err(CompileFailed)?;
            Ok((program, declared))
        }
        InputFormat::CastJson | InputFormat::CastBinary => {
            // Gated load (`cast_version` major), then the `.crush` pipeline.
            let mut cast = formats::load_cast(path, format)?;
            let declared = cast.manifest.as_ref().and_then(|m| m.capabilities.clone());
            crush_frontend::import_system::resolve_imports(&mut cast, path, &import_options(lib_paths))?;
            let program = crush_lang_sdk::compile::compile_cast_program(
                cast,
                &crush_lang_sdk::compile::CompileOptions::default(),
            )
            .map_err(CompileFailed)?;
            Ok((program, declared))
        }
        InputFormat::CasmJson | InputFormat::CasmBinary => {
            // Gated load (`version` major); the CASM manifest declares the caps.
            let casm = formats::load_casm(path, format)?;
            let program = crush_lang_sdk::compile::casm_to_vm(&casm)?;
            let declared = program.manifest.declared_capabilities.clone();
            Ok((program, declared))
        }
        InputFormat::Cvms => {
            let source = std::fs::read_to_string(path)?;
            // The text's `.cap` lines declare what it needs; `--cap` adds to them.
            let mut permissions: Vec<&str> = casm_caps.iter().map(|s| s.as_str()).collect();
            // Hand-written assembly has no compiler to declare its polyglot grants, and
            // EXEC_LANG checks the manifest (CRUSH-226) — `--polyglot` declares them too,
            // so it keeps meaning what it meant before.
            if polyglot {
                permissions.extend(["polyglot.python", "polyglot.javascript", "polyglot.bash"]);
            }
            Ok((crush_lang_sdk::assemble(&source, Some(&permissions), None)?, None))
        }
        InputFormat::Cvm1 => {
            let program = crush_vm::Program::from_blob(&std::fs::read(path)?)?;
            let declared = program.manifest.declared_capabilities.clone();
            Ok((program, declared))
        }
    }
}

/// The import search path: `-L` directories, then `CRUSH_PATH`.
fn import_options(lib_paths: &[PathBuf]) -> crush_frontend::import_system::ImportOptions {
    crush_frontend::import_system::ImportOptions::with_crush_path(lib_paths.iter().cloned())
}

/// `crush-run caps FILE`: what the program can use, grouped by what grants
/// it, without running it.
fn show_program_caps(path: &std::path::Path, json: bool, lib_paths: &[PathBuf]) -> anyhow::Result<()> {
    let (program, declared) = load_program(path, &[], false, lib_paths, true)?;
    let used = crush_lang_sdk::effects::used_by(&program)?;
    let catalog = crush_lang_sdk::effects::catalog();
    let grant_of = |cap: &str| -> &'static str {
        if crush_vm::capabilities().contains_key(cap) {
            return "portable";
        }
        catalog.iter().find(|c| c.name == cap).map_or("not available in this host", |c| c.grant)
    };
    let ambient = |grant: &str| matches!(grant, "portable" | "always" | crush_lang_sdk::effects::STDLIB_GRANT);
    if json {
        let caps: Vec<_> = used
            .iter()
            .map(|c| {
                let grant = grant_of(c);
                serde_json::json!({"name": c, "grant": grant, "ambient": ambient(grant)})
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "program": path.display().to_string(),
                "declared": declared,
                "uses": caps,
            }))
            .expect("serializable")
        );
        return Ok(());
    }
    println!("{}", path.display());
    match &declared {
        Some(d) if d.is_empty() => println!("  declares: nothing beyond the ambient capabilities"),
        Some(d) => println!("  declares: {}", d.join(", ")),
        None => println!("  declares: (no @capabilities declaration)"),
    }
    let mut needs: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    let mut free = Vec::new();
    for cap in &used {
        let grant = grant_of(cap);
        if ambient(grant) {
            free.push(cap.as_str());
        } else {
            needs.entry(grant).or_default().push(cap);
        }
    }
    if needs.is_empty() {
        println!("  needs no grants");
    } else {
        println!("  needs these grants to run:");
        for (grant, caps) in &needs {
            println!("    {grant}: {}", caps.join(", "));
        }
    }
    if !free.is_empty() {
        println!("  also uses (ambient): {}", free.join(", "));
    }
    Ok(())
}

/// A `.crush` source that failed to parse, type-check or compile, so `main`
/// labels it `[compile]` rather than `[runtime]` (CRUSH-127). Display and the
/// cause chain are the inner error's, unchanged.
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

/// `crush-run caps --json`: every capability this build can register, with
/// its effects (CRUSH-155) and the flag that grants it.
fn list_caps_json() {
    let list: Vec<_> = crush_lang_sdk::effects::catalog()
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name, "argc": c.argc, "returns": c.returns,
                "effects": c.effects, "grant": c.grant,
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&list).expect("serializable")
    );
}

fn list_caps() {
    println!("Built-in portable capabilities:");
    println!("  io.print      write args to stdout");
    println!("  io.read       read one line from stdin (EOF → empty string)");
    println!("  str.concat    concatenate args → string");
    println!("  str.len       byte length of a string");
    println!();
    println!("Host capabilities (enable with --fs / --env / --time):");
    println!("  fs.read PATH           read file contents");
    println!("  fs.write PATH DATA     write file contents");
    println!("  fs.exists PATH         return 1 if file exists, else 0");
    println!("  fs.list DIR            list directory entries (sorted)");
    println!("  fs.ls [DIR]            same as fs.list; DIR defaults to the working directory");
    println!("  fs.cat PATH            same as fs.read");
    println!(
        "  fs.pwd                 working directory, relative to --fs-root (\".\" at the root)"
    );
    println!(
        "  fs.cd DIR              move the working directory (VM-local, never leaves --fs-root)"
    );
    println!("  fs.mkdir PATH [PARENTS]  create a directory (PARENTS=true: like mkdir -p)");
    println!("  fs.rm PATH [RECURSIVE]   remove a file; a directory needs RECURSIVE=true");
    println!("  fs.cp SRC DST [RECURSIVE]  copy a file; a directory needs RECURSIVE=true");
    println!("  fs.mv SRC DST          move / rename");
    println!("  fs.touch PATH          create an empty file or update its mtime");
    println!("  fs.find DIR [GLOB]     every entry below DIR, optionally filtered by a name glob");
    println!("  text.head/tail PATH N  first / last N lines of a file");
    println!("  text.wc PATH           {{lines, words, chars}} of a file");
    println!("  text.cut PATH DELIM COL  1-based column of every line");
    println!("  text.grep PAT PATH [RECURSIVE [IGNORE_CASE]]  (also needs the stdlib feature)");
    println!("  env.get NAME           read environment variable");
    println!("  env.all                map of every environment variable");
    println!("  env.home_dir           home directory (HOME / USERPROFILE), or null");
    println!("  time.now               return Unix timestamp in SECONDS (exosphere's time.now was ms: use time.now_ms)");
    println!("  time.now_ms / now_iso  current time (epoch MILLISECONDS / RFC 3339)");
    println!("  time.elapsed START_MS  milliseconds since START_MS");
    println!("  time.sleep MS          block for MS milliseconds (bounded by the wall-time quota)");
    println!("  async.sleep MS         same as time.sleep (blocking; does not yield)");
    println!("Message-bus capabilities (enable with --bus):");
    println!("  message_bus.publish TOPIC PAYLOAD");
    println!("  message_bus.subscribe TOPIC");
    println!("  message_bus.recv       blocking receive");
    println!("Task capabilities (enable with --task; task.start spawns a host process, granting process/spawn):");
    println!("  task.start NAME COMMAND [ARGS...]");
    println!("  task.stop TASK_ID");
    println!("  task.list");
    println!("Knowledge-graph capabilities (enable with --akg):");
    println!("  akg.write ID JSON_UNIT");
    println!("  akg.read ID");
    println!("  akg.search QUERY");
    println!("Codebase capabilities (enable with --codebase FILE_OR_DIR, repeatable; read-only):");
    println!("  codebase.modules / .definition NAME / .callers NAME / .invariants NAME");
    println!("  codebase.exhaustive_sites NAME / .uncovered_paths NAME / .wip / .temporaries");
    println!("  codebase.stale_temporaries / .decisions / .annotation_history NAME");
    println!("Process capabilities (enable with --process):");
    println!("  process.exec CMD [ARGS...]  run subprocess, return {{exit_code, stdout, stderr}}");
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
        println!("  net.http_put URL BODY  HTTP PUT request");
        println!("  net.http_delete URL    HTTP DELETE request");
        println!(
            "  net.http_request METHOD URL BODY HEADERS  any method; returns {{status, body}}"
        );
    }
    #[cfg(feature = "tcp")]
    {
        println!("Raw TCP client capabilities (enable with --tcp; NOT granted by --net):");
        println!("  tcp.connect URI        connect to tcp://IP:PORT, return a connection id");
        println!("  tcp.tls_wrap ID SNI    upgrade a connection to TLS (client side)");
        println!("  tcp.send ID DATA       write a string, return bytes written");
        println!("  tcp.recv ID            blocking read, return a string");
        println!("  tcp.close ID           drop the connection");
        println!("  tcp.ping X             returns \"pong\"");
        println!(
            "Raw TCP server capabilities (enable with --tcp-listen; a separate, stronger grant):"
        );
        println!("  tcp.listen URI         bind tcp://IP:PORT, return a listener id");
        println!("  tcp.accept LISTENER    blocking accept, return a connection id");
        println!("  (tcp.send/recv/close/ping also work under --tcp-listen; tcp.connect does not)");
    }
    #[cfg(feature = "db")]
    {
        println!("Database capabilities (enable with --db PATH):");
        println!("  db.query SQL [PARAMS...]  execute SELECT, return rows");
        println!("  db.execute SQL [PARAMS...] execute INSERT/UPDATE/DELETE, return affected rows");
    }
    #[cfg(not(feature = "db"))]
    {
        println!("Database capabilities (db.query, db.execute) are not in this build:");
        println!("  they are opt-in; rebuild with `--features db` (e.g.");
        println!("  `cargo install crush-lang-sdk --features db`), then run with --db PATH.");
    }
    #[cfg(not(feature = "net"))]
    {
        println!("Network capabilities (net.http_*) are not in this build:");
        println!("  rebuild with `--features net`, then run with --net.");
    }
    #[cfg(feature = "stdlib")]
    {
        println!("Standard library capabilities (on by default; --no-stdlib to disable):");
        println!(
            "  str.len/split/join/trim/replace/contains/starts_with/ends_with/to_upper/to_lower"
        );
        println!("  str.pad_left/pad_right/repeat/substring/char_at/index_of/format");
        println!("  math.sqrt/abs/floor/ceil/round/sin/cos/tan/pow/min/max/pi");
        println!("  conv.to_int/to_float/to_str/to_bool/parse_int/parse_float/type_of");
        println!("  collections.len/reverse/includes/flatten/chunk/zip/unique");
        println!("  collections.keys/values/entries/merge/pluck/sort_by/find/any/all");
        println!("  json.parse/stringify/stringify_pretty");
        println!("  path.join/dirname/basename/extension/is_absolute/normalize/stem");
        println!("  regex.test/match/find_all/replace/split");
        println!("  bytes.len/slice/from_string/to_string");
        println!("  buffer.alloc/write/read/freeze");
        println!("  binary.read_u16_le/u32_le/u64_le/u16_be/u32_be/u64_be (+ write_*)");
        println!("  result.ok/err/is_ok/unwrap");
        println!("  text.sort/uniq  time.format/parse  env.os/arch");
        println!("  system.path_normalize/format_info/format_node_info  (SBL, written in Crush)");
    }
}

fn run_file(args: &RunArgs) -> anyhow::Result<()> {
    use crush_lang_sdk::formats::{self, InputFormat};
    // Pre-validate the extension so feature-gate warnings (e.g. the
    // "--graphics requires the 'graphics' feature" hint) don't fire for
    // an unsupported file extension -- originally the extension dispatch
    // ran before quotas/caps were built, so unknown extensions bailed
    // cleanly. Hoisting Runtime init above the dispatch would otherwise
    // reorder those warnings before the bail.
    // (The deprecation note for an old name is printed by `load_program`.)
    let format = match formats::detect(&args.path)? {
        Some(d) => d.format,
        None => {
            formats::detect_or_bail(&args.path, false)?;
            unreachable!()
        }
    };

    // Build Runtime up-front so the `.cvm1` arm can route through typed
    // `Runtime::run_blob` (which loads *and* executes), mapping
    // `Program::from_blob` errors to `RuntimeError::LoadBlob` →
    // `E-RT01` in JSON diagnostic mode. The previous shape called
    // `crush_vm::Program::from_blob` directly, whose `CrushError`
    // blocked the downcast in `main::main` and fell back to generic
    // `E-IO` — so `E-RT01` was unreachable from this binary.
    //
    // The `.crush` and `.casm` arms keep an explicit compile-then-run
    // shape because no `Runtime::run_*` wrapper exists yet for them
    // (`CODE_RT_ASSEMBLY` / `CODE_RT_INDEX_PARSE` / `CODE_RT_INDEX_READ`
    // remain future work — see `JsonDiagnostic` in `theme.rs`).
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
        .polyglot(if args.polyglot { &["python", "javascript", "bash"] } else { &[] })
        .fs(args.fs)
        .fs_root(args.fs_root.to_string_lossy())
        .env(args.env)
        .time(args.time)
        .bus(args.bus)
        .task(args.task)
        .akg(args.akg)
        .process(args.process)
        .crypto(args.crypto);
    if !args.codebase.is_empty() {
        builder = builder.codebase(
            crush_lang_sdk::runtime::index_files(&args.codebase).map_err(|e| anyhow::anyhow!("--codebase: {e}"))?,
        );
    }

    // `gpu.launch` compiles kernels from the program's CASM, which the CVM1
    // program doesn't keep — so give the capability its own compile of the source.
    #[cfg(feature = "gpu")]
    if args.gpu {
        builder = builder.gpu(true);
        if format == InputFormat::Crush {
            let (casm, _) = crush_lang_sdk::compile::compile_crush_file_to_casm(
                &args.path,
                &import_options(&args.lib_paths),
                &crush_lang_sdk::compile::CompileOptions::default(),
            )
            .map_err(CompileFailed)?;
            builder = builder.gpu_program(std::sync::Arc::new(casm));
        }
    }
    #[cfg(not(feature = "gpu"))]
    if args.gpu {
        anyhow::bail!("--gpu requires the 'gpu' feature, which this build of crush-run was compiled without");
    }

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

    #[cfg(feature = "tcp")]
    {
        builder = builder.tcp(args.tcp).tcp_listen(args.tcp_listen);
    }
    #[cfg(not(feature = "tcp"))]
    if args.tcp || args.tcp_listen {
        eprintln!(
            "warning: --tcp/--tcp-listen require the 'tcp' feature (not enabled in this build)"
        );
    }

    #[cfg(feature = "db")]
    if let Some(ref db_path) = args.db {
        builder = builder.db(db_path.to_string_lossy());
    }
    #[cfg(not(feature = "db"))]
    if args.db.is_some() {
        eprintln!(
            "warning: --db requires the 'db' feature, which is opt-in and not in this build \
             (rebuild with `--features db`)"
        );
    }

    // Stdcaps are pure (no I/O, no authority), so they are on unless the
    // caller opts out (CRUSH-113). Asking for them in a build that lacks them
    // is an error rather than a warning followed by "unknown capability".
    #[cfg(feature = "stdlib")]
    {
        builder = builder.stdlib(!args.no_stdlib);
    }
    #[cfg(not(feature = "stdlib"))]
    if args.stdlib {
        anyhow::bail!(
            "--stdlib requires the 'stdlib' feature, which this build of crush-run was compiled without"
        );
    }

    let runtime =
        Runtime::with_quotas(quotas).with_host_caps(builder.args(args.script_args.clone()).build());

    // CRUSH-243: refuse before anything runs when the program can request a
    // capability this run doesn't grant, listing all of them at once. A
    // `.cvm1` blob that doesn't decode is left to `run_blob` below, which
    // reports it as a typed load error.
    let preflight = match format {
        InputFormat::Cvm1 => std::fs::read(&args.path)
            .ok()
            .and_then(|blob| crush_vm::Program::from_blob(&blob).ok()),
        _ => {
            let (program, _declared) = load_program(
                &args.path,
                &args.caps,
                args.polyglot,
                &args.lib_paths,
                matches!(args.message_format, MessageFormat::Text),
            )?;
            // CRUSH-369: annotation warnings (stale @wip, expired @temporary,
            // wildcard in an exhaustive match, @must-call-after): advisory,
            // stderr, in the same format as the other messages.
            if format == InputFormat::Crush {
                if let Ok(parsed) = crush_frontend::parse_file(&args.path, &import_options(&args.lib_paths)) {
                    crush_lang_sdk::compile::print_annotation_diagnostics(
                        &crush_lang_sdk::compile::annotation_diagnostics(&parsed, &args.path.display().to_string()),
                        matches!(args.message_format, MessageFormat::Json),
                    );
                }
            }
            // Compile warnings (CRUSH-270's polyglot implicit-output/clobber
            // warnings) go to stderr, never the program's stdout.
            for warning in &program.warnings {
                eprintln!("warning: {warning}");
            }
            Some(program)
        }
    };
    if let Some(program) = &preflight {
        let missing = runtime.missing_grants(program)?;
        if !missing.is_empty() {
            return Err(NotGranted::new(&args.path, &missing).into());
        }
    }

    // Output goes to stdout as the program prints it, so a long-running script
    // shows progress and nothing printed before an error is lost.
    let mut write_out = |part: &str| {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(part.as_bytes());
        let _ = out.flush();
    };
    let result = match &preflight {
        Some(program) => runtime.run_streaming(program, &mut write_out),
        // A `.cvm1` that doesn't decode: `run_blob` reports it as a typed
        // `RuntimeError::LoadBlob` (E-RT01 in JSON mode).
        None => runtime.run_blob(&std::fs::read(&args.path)?),
    };
    let result = match result {
        // `sys.exit(code)`: a requested stop, not an error.
        Err(crush_lang_sdk::RuntimeError::Vm(crush_vm::VmError::Exit(code))) => {
            std::process::exit(code)
        }
        other => other?,
    };

    print_result(&result);
    if !result.halted {
        eprintln!("(program fell off end without HALT)");
    }
    Ok(())
}

fn print_result(result: &VmResult) {
    // The output itself was streamed while the program ran.
    eprintln!("[steps={}, stack={}]", result.steps, result.stack.len());
}
