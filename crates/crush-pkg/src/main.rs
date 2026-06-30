#![allow(dead_code)]

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

use std::io::Write;

use crush_diagnostics::{diag_line, strict_downgrade, DiagRecord};

// =========================================================================
// Output format enum — third `value_name = "FORMAT"` value alongside
// `text` and `json`.
//
// The `Strict` variant downgrades `level: "note"` records to
// `level: "error"` at emit time so non-fatal builder warnings
// (e.g. future dead-code detection in `capsule.toml`) break the
// build without a per-call-site change. Useful as a CI gate. The
// downgrade is implemented in [`strict_downgrade`] so the rule
// lives in one place and is testable independent of the wire.
// =========================================================================

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageFormat {
    /// Default human-readable text format (the pre-existing
    /// stdout/println! + stderr/eprintln! shape).
    Text,
    /// NDJSON output for editor/CI consumers. `level: "note"`
    /// records pass through unchanged.
    Json,
    /// NDJSON with note-level builder warnings downgraded to
    /// `level: "error"` so the build fails. CI gate. Editors and
    /// CI consumers still see the seven-field wire shape; the
    /// `level` slot is what changes.
    Strict,
}

// Modules are declared in `src/lib.rs` as `pub mod ...` so the lib
// facade exposes them. The bin and lib are SEPARATE crates in this
// Cargo package, so `main.rs` reaches them through the lib's external
// crate name (`crush_pkg::builder::*`, etc.) — unqualified
// `builder::*` paths don't resolve from the bin's crate root.
//
// Without the qualifier this file was failing with E0432/E0433
// ("unresolved import" / "unresolved module") because `main.rs`
// is the bin crate root, not a node in the lib's module tree. The
// lib re-exports its modules as `pub mod`, but those modules are
// reachable from outside the lib ONLY through `crush_pkg::` (the
// lib crate's external name). Closing
// `TICKETS/CRUSH-SELFHOST-1.md#constraint-4` was the motivation
// for adding `src/lib.rs` in the first place: the integration test
// `tests/test_selfhost_demo.rs` already uses `crush_pkg::*` paths;
// `main.rs` joins the same external-name idiom here so the bin and
// the test exercise the same module tree.
use crush_pkg::builder::PackageBuilder;
use std::collections::HashSet;

use crush_pkg::manifest::Manifest;
use crush_pkg::packer::{pack, unpack};
use crush_pkg::runners::{ExecutionResult, get_runner_for_payload};
use crush_pkg::signer::{generate_keys, sign_package, verify_package};

fn find_manifest() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();
    loop {
        for name in ["capsule.toml", "Capsule.toml", "crush.toml", "Crush.toml"] {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            anyhow::bail!(
                "no capsule/crush.toml found in {} or any parent directory",
                cwd.display()
            );
        }
    }
}

fn load_manifest() -> anyhow::Result<(Manifest, PathBuf)> {
    let path = find_manifest()?;
    let manifest = Manifest::from_file(&path)?;
    let root = path.parent().unwrap().to_path_buf();
    Ok((manifest, root))
}

#[derive(Parser)]
#[command(name = "crush-pkg")]
#[command(about = "Crush Package Manager — build, run, and manage Crush programs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Output format for terminal messages. `text` default;
    /// `json` for editor/CI consumers; `strict` for NDJSON + CI
    /// gate that downgrades `level: "note"` builder warnings to
    /// `level: "error"` so the build fails. `global = true`
    /// lets editors pass the flag before OR after the
    /// subcommand. Matches the `--message-format=FORMAT`
    /// dispatch wired into `crush`, `crushc`, `crush-run`,
    /// `crush-compile`, `crush-repl`, `xtask`, `crush-vm`, and
    /// `crush-installer`.
    #[arg(long, global = true, value_name = "FORMAT")]
    message_format: Option<MessageFormat>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new Crush package
    New {
        /// Package name
        name: String,
        /// Output directory (defaults to ./<name>)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
    /// Build the current package
    Build,
    /// Run the current package
    Run {
        /// Arguments passed to the program's main function
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Type-check without emitting bytecode
    Check,
    /// Pack source into a .crush-pack archive
    Pack {
        /// Output path (defaults to <name>.crush-pack)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Unpack a .crush-pack archive
    Unpack {
        /// Path to .crush-pack archive
        pack: PathBuf,
        /// Output directory (defaults to ./<name>)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
    /// Generate Ed25519 signing keys
    GenerateKeys {
        /// Output directory for key files
        #[arg(default_value = "./keys")]
        dir: PathBuf,
    },
    /// Sign a .cap file with a private key
    Sign {
        /// Path to the .cap file
        package: PathBuf,
        /// Path to private key (64-byte keypair)
        key: PathBuf,
    },
    /// Verify a .cap file's signature
    Verify {
        /// Path to the .cap file
        package: PathBuf,
        /// Path to public key (32 bytes)
        key: PathBuf,
    },
    /// Bundle a directory of static web assets into a signed .ecap site capsule
    Site {
        /// Directory of static web assets (html/css/js/…)
        dir: PathBuf,
        /// Capsule name
        #[arg(long)]
        name: String,
        /// Capsule version
        #[arg(long, default_value = "0.1.0")]
        version: String,
        /// Entry document (relative path within the assets dir)
        #[arg(long, default_value = "index.html")]
        entry: String,
        /// Output .ecap path (defaults to <name>.ecap)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Optional Ed25519 private key (64-byte keypair) to sign the manifest
        #[arg(long)]
        sign: Option<PathBuf>,
        /// Signer DID recorded in the signature
        #[arg(long)]
        did: Option<String>,
    },
    /// Extract (and hash-verify) a static-site .ecap capsule to a directory
    SiteExtract {
        /// Path to the .ecap site capsule
        capsule: PathBuf,
        /// Output directory
        #[arg(short, long, default_value = "./site")]
        dir: PathBuf,
    },
    /// Show package metadata
    Show,
    /// Lint only — run the dead-code detector against the
    /// manifest without a full `build`. Useful for editor
    /// pre-build invocation: surface warnings via the same
    /// `--message-format=FORMAT` dispatch (`text`/`json`/`strict`).
    /// Under `--message-format=strict`, ANY finding exits 1
    /// (CI gate — distinct from the post-build soft-emit on
    /// `build`/`check`).
    Lint {},
}

// =========================================================================
// Per-domain wire codes for crush-pkg failure paths.
//
// Domain codes (`E-NEW`, `E-MANIFEST`, `E-BUILDER`, `E-RUN`,
// `E-SIGN`, `E-SITE`) are tagged via [`CommandFailure`] so a
// single `dispatch()` return point routes the failure to the right
// code without a per-call-site JSON emit.
//
// The seven-field wire SHAPE (DiagRecord + diag_line_from) is
// absorbed from the canonical peer crate
// `crush_diagnostics` — no per-binary struct copy here. Per-binary
// code-value lockdown lives in
// `crush_diagnostics::tests::wire_format::diag_record_accepts_all_per_binary_wire_codes`
// (so a re-order or rename of the canonical surface synchronously
// fails against this list). The seven-field byte-exact shape is
// locked in the same test module. Adding a new domain-code is a
// one-line const + a new match arm; bring it into the canonical
// literal list in the cross-binary lockdown so the byte-exact
// surface covers it.
// =========================================================================

/// Wire codes emitted by `crush-pkg` failure paths, mapped by
/// domain. Inline literal paralleling the `E-AUDIT`/`E-LINT`
/// convention documented in
/// `crush_lang_sdk/src/theme.rs::JsonDiagnostic`.
pub const CODE_NEW: &str = "E-NEW";
pub const CODE_MANIFEST: &str = "E-MANIFEST";
pub const CODE_BUILDER: &str = "E-BUILDER";
pub const CODE_RUN: &str = "E-RUN";
pub const CODE_SIGN: &str = "E-SIGN";
pub const CODE_SITE: &str = "E-SITE";
/// `crush-pkg lint` subcommand failure-path code. Distinct from
/// `E-BUILDER` so dispatch routes the lint subcommand's failure
/// (under strict-mode CI gate, or when the manifest is
/// unreadable) to this code while per-finding records stay on
/// `E-BUILDER`. Already present in the canonical cross-binary
/// lockdown list (`xtask lint-dejavue` used `E-LINT`
/// previously), so adding the const here slots the new code into
/// the byte-exact surface without patching `crush_diagnostics`.
pub const CODE_LINT: &str = "E-LINT";

/// Tagged-error wrapper that lets `dispatch` route failures to
/// the right per-domain wire code. Each variant carries the full
/// anyhow::Error display format (`{e:#}` for context chain) so
/// the JSON consumer sees both the code AND the underlying chain
/// in the `message` field.
enum CommandFailure {
    New(String),
    Manifest(String),
    Builder(String),
    Run(String),
    Sign(String),
    Site(String),
    /// `crush-pkg lint` subcommand failure-path tag. Triggered
    /// when the strict-mode CI gate trips (any dead-code finding
    /// exits 1) or when the manifest is unreadable. Distinct
    /// from `Builder` so dispatch routes each domain to its
    /// right wire code.
    Lint(String),
}

impl CommandFailure {
    fn code_and_message(&self) -> (&'static str, &str) {
        match self {
            CommandFailure::New(m) => (CODE_NEW, m),
            CommandFailure::Manifest(m) => (CODE_MANIFEST, m),
            CommandFailure::Builder(m) => (CODE_BUILDER, m),
            CommandFailure::Run(m) => (CODE_RUN, m),
            CommandFailure::Sign(m) => (CODE_SIGN, m),
            CommandFailure::Site(m) => (CODE_SITE, m),
            CommandFailure::Lint(m) => (CODE_LINT, m),
        }
    }
}

/// Note: `strict_downgrade` was the local CI-gate kernel defined
/// here before; it has been promoted to the canonical peer crate
/// `crush_diagnostics` (alongside `diag_line` / `diag_line_from`
/// / `wants_json`) so any future binary adopting
/// `--message-format=strict` — `xtask`, `crush-vm`,
/// `crush-installer`, the future `crush-lint` over `.dejavue` —
/// routes through the same kernel rather than re-deriving the
/// `note` → `error` lift. See
/// `crush_diagnostics::strict_downgrade` for the canonical
/// surface; the four lockdown tests in
/// `crush_diagnostics/tests/wire_format.rs` pin its behaviour.

/// Emit one NDJSON line (newline-terminated) on the writer.
/// Strict-mode downgrade is applied at emit time so the
/// canonical [`strict_downgrade`] is the single source of truth
/// for the CI-gate behavior; callers cannot accidentally bypass
/// it.
///
/// `out: &mut impl Write` (rather than `&mut dyn Write`) lets the
/// compiler monomorphize per call site: production uses
/// `&mut std::io::stdout()`; tests use `&mut Vec<u8>` for
/// byte-exact capture without touching the global stdout fd.
/// Returns `std::io::Result<()>` so write errors propagate via
/// anyhow's `From<io::Error>` (instead of being silently dropped
/// via the prior `print!` shape).
///
/// Routes through the struct form (`DiagRecord` + `diag_line`)
/// rather than `diag_line_from` so that callers passing a source-
/// file line number (the post-dispatch lint path; tests that
/// verify the four-tuple wire shape) actually preserve `line`
/// through the canonical wire. `diag_line_from` hardcodes
/// `line: None` for the function-form shorthand, so the previous
/// `emit_diag(out, code, level, msg, file, hint, strict)` shape
/// silently dropped line info — fixed by adding `line: Option<u32>`
/// between `file` and `hint`, then collapsing both call paths
/// (failure-path / lint-findings) onto the struct form here.
fn emit_diag(
    out: &mut impl Write,
    code: &str,
    level: &str,
    message: &str,
    file: Option<&str>,
    line: Option<u32>,
    hint: Option<&str>,
    strict_mode: bool,
) -> std::io::Result<()> {
    let final_level = strict_downgrade(level, strict_mode);
    let rec = DiagRecord {
        code,
        level: final_level,
        file,
        line,
        col: None,
        message,
        hint,
    };
    out.write_all(diag_line(&rec).as_bytes())
}

fn main() {
    let cli = Cli::parse();
    let json_mode = matches!(
        cli.message_format,
        Some(MessageFormat::Json) | Some(MessageFormat::Strict),
    );
    let strict_mode = matches!(cli.message_format, Some(MessageFormat::Strict));
    // Capture the subcommand BEFORE dispatch takes ownership of
    // `cli` — used to gate the post-dispatch lint pass so it
    // doesn't double-emit when the user is running the `lint`
    // subcommand itself (the lint subcommand already emits via
    // dispatch → `handle_lint`; re-running post-dispatch here
    // would duplicate per-finding records on stdout).
    let is_lint_subcommand = matches!(cli.command, Commands::Lint { .. });
    if let Err(failure) = dispatch(cli, json_mode, strict_mode) {
        let (code, msg) = failure.code_and_message();
        if json_mode {
            let mut out = std::io::stdout();
            // Failure-path emit is `level: "error"` (errors promote
            // themselves; strict mode never demotes). Strict-mode
            // threading is preserved so future strict-mode `note`
            // sites (e.g. `CommandFailure::Lint("...")` for
            // future dead-code-in-capsule-toml detection) honour
            // the CI gate. Discarding the write error is
            // acceptable: we're exiting 1 anyway and a broken
            // stdout pipeline shouldn't panic before the process
            // termination.
            let _ = emit_diag(&mut out, code, "error", msg, None, None, None, strict_mode);
        } else {
            eprintln!("crush-pkg[{code}]: {msg}");
        }
        std::process::exit(1);
    }
    // ----------------------------------------------------------------
    // Post-dispatch lint pass (Option C from thinker's design).
    //
    // After `dispatch` succeeds the manifest is known to exist on
    // disk (a missing manifest would have surfaced as
    // `CommandFailure::Manifest` from the handler). Run the
    // `capsule.toml` dead-code detector and emit one NDJSON
    // record per finding. Strict-mode honored automatically via
    // `emit_diag` → `strict_downgrade`. Discarding write errors
    // mirrors the failure path above (a broken stdout pipeline
    // shouldn't panic before process termination).
    //
    // Gated to skip for the `lint` subcommand (which already
    // emits via dispatch → `handle_lint`) — without this gate,
    // `crush-pkg lint --message-format=json` would double-emit
    // per-finding records on stdout.
    // ----------------------------------------------------------------
    if !is_lint_subcommand {
        if let Ok(manifest_path) = find_manifest() {
            if let Some(name_only) = manifest_path.file_name() {
                // Only lint the canonical `capsule.toml` (and its
                // case-variants picked by `find_manifest`) — `crush.toml`
                // is the legacy alias and is exempt from this lint for
                // now (its `[env]` semantics differ across versions).
                if name_only == "capsule.toml" || name_only == "Capsule.toml" {
                    let mut out = std::io::stdout();
                    let _ = emit_post_dispatch_lint(
                        &mut out,
                        Some(manifest_path.as_path()),
                        json_mode,
                        strict_mode,
                    );
                }
            }
        }
    }
}

/// Emit one canonical wire record per `capsule.toml` dead-code
/// finding. Strict-mode honored automatically via [`emit_diag`]
/// → [`strict_downgrade`]. Returns the number of findings
/// emitted so callers can take action (currently: we discard
/// the count; future: a `--deny-warnings` flag would `exit(1)`
/// on count > 0 under strict mode).
///
/// `manifest_path: Option<&Path>` is threaded explicitly so tests
/// can pass a fixture path without depending on cwd. `None`
/// silently produces zero emissions — the dispatch path already
/// surfaces manifest load errors via `CommandFailure::Manifest`,
/// so re-erroring here would double-report.
/// `&mut impl Write` (rather than `&mut dyn Write`) lets the
/// compiler monomorphize per call site: production uses
/// `&mut std::io::stdout()`; tests use `&mut Vec<u8>` for
/// byte-exact capture without touching the global stdout fd.
fn emit_post_dispatch_lint(
    out: &mut impl Write,
    manifest_path: Option<&Path>,
    json_mode: bool,
    strict_mode: bool,
) -> std::io::Result<usize> {
    let manifest_path = match manifest_path {
        Some(p) => p,
        None => return Ok(0),
    };
    let content = match std::fs::read_to_string(manifest_path) {
        Ok(c) => c,
        Err(_) => return Ok(0),
    };
    // Resolve the entry-file cross-reference set so the lint
    // dispatcher can run rules that depend on it (currently the
    // `[dependencies]` unreachable-name cross-reference pass).
    // Graceful-degradation contract: if anything fails here
    // (manifest unparseable / no entry field / entry file
    // unreadable), pass `None` and the dispatcher skips the
    // entry-aware rule rather than flagging every dep.
    let entry_refs: Option<HashSet<String>> =
        Manifest::from_str(&content, manifest_path)
            .ok()
            .and_then(|manifest| {
                let root = manifest_path.parent()?;
                let entry_path = root.join(&manifest.capsule.entry);
                crush_pkg::builder::scan_entry_file_references(&entry_path)
            });
    let findings = crush_pkg::builder::lint_capsule_toml_with_entry(&content, entry_refs.as_ref());
    if json_mode {
        for f in &findings {
            emit_diag(
                out,
                crush_pkg::builder::LintFinding::CODE,
                "note",
                &f.message,
                Some("capsule.toml"),
                Some(f.line),
                Some(&f.hint),
                strict_mode,
            )?;
        }
    } else {
        // Text mode keeps the canonical `note:` + `dead-code:`
        // prefix for grep-ability + human readability, then prints
        // the rule-intent message verbatim (single source of truth).
        for f in &findings {
            writeln!(
                out,
                "note: capsule.toml:{}: dead-code: {}",
                f.line, f.message
            )?;
        }
    }
    Ok(findings.len())
}

fn dispatch(
    cli: Cli,
    json_mode: bool,
    strict_mode: bool,
) -> Result<(), CommandFailure> {
    match cli.command {
        Commands::New { name, dir } => handle_new(name, dir)
            .map_err(|e| CommandFailure::New(format!("{e:#}"))),
        Commands::Build => handle_build()
            .map_err(|e| CommandFailure::Builder(format!("{e:#}"))),
        Commands::Run { args } => handle_run(args, strict_mode)
            .map_err(|e| CommandFailure::Run(format!("{e:#}"))),
        Commands::Check => handle_check()
            .map_err(|e| CommandFailure::Builder(format!("{e:#}"))),
        Commands::Pack { output } => handle_pack(output)
            .map_err(|e| CommandFailure::Manifest(format!("{e:#}"))),
        Commands::Unpack { pack, dir } => handle_unpack(pack, dir)
            .map_err(|e| CommandFailure::Manifest(format!("{e:#}"))),
        Commands::GenerateKeys { dir } => handle_keygen(dir)
            .map_err(|e| CommandFailure::Sign(format!("{e:#}"))),
        Commands::Sign { package, key } => handle_sign(package, key)
            .map_err(|e| CommandFailure::Sign(format!("{e:#}"))),
        Commands::Verify { package, key } => handle_verify(package, key)
            .map_err(|e| CommandFailure::Sign(format!("{e:#}"))),
        Commands::Site {
            dir,
            name,
            version,
            entry,
            output,
            sign,
            did,
        } => handle_site(dir, name, version, entry, output, sign, did)
            .map_err(|e| CommandFailure::Site(format!("{e:#}"))),
        Commands::SiteExtract { capsule, dir } => handle_site_extract(capsule, dir)
            .map_err(|e| CommandFailure::Site(format!("{e:#}"))),
        Commands::Show => handle_show()
            .map_err(|e| CommandFailure::Manifest(format!("{e:#}"))),
        Commands::Lint {} => handle_lint(json_mode, strict_mode)
            .map_err(|e| CommandFailure::Lint(format!("{e:#}"))),
    }
}

fn handle_new(name: String, dir: Option<PathBuf>) -> anyhow::Result<()> {
    let target = dir.unwrap_or_else(|| PathBuf::from(&name));
    if target.exists() {
        anyhow::bail!("directory {} already exists", target.display());
    }
    let manifest = crush_pkg::manifest::scaffold_package(&target, &name)?;
    println!(
        "created new Crush package at {}",
        target.join("capsule.toml").display()
    );
    println!("  name:    {}", manifest.capsule.name);
    println!("  entry:   {}", manifest.capsule.entry);
    println!("  version: {}", manifest.capsule.version);
    Ok(())
}

fn handle_build() -> anyhow::Result<()> {
    let (manifest, root) = load_manifest()?;
    println!(
        "building {} v{}",
        manifest.capsule.name, manifest.capsule.version
    );
    let builder = PackageBuilder::new(manifest, root);
    let output = builder.build()?;
    builder.write_output(&output)?;
    println!(
        "done: {} function(s), {} byte(s)",
        output.functions.len(),
        output.program.code.len()
    );
    Ok(())
}

fn handle_run(args: Vec<String>, strict_mode: bool) -> anyhow::Result<()> {
    let (manifest, root) = load_manifest()?;
    let payload = root.join(&manifest.capsule.entry);
    if !payload.exists() {
        anyhow::bail!("entry file not found: {}", payload.display());
    }

    let mut format = crush_pkg::manifest::PayloadFormat::from_path(&payload);
    if format == crush_pkg::manifest::PayloadFormat::Unknown {
        if let Ok(bytes) = std::fs::read(&payload) {
            let detected = crush_pkg::manifest::PayloadFormat::from_magic(&bytes);
            if detected != crush_pkg::manifest::PayloadFormat::Unknown {
                format = detected;
            }
        }
    }

    if format == crush_pkg::manifest::PayloadFormat::Unknown && strict_mode {
        let magic_bytes = std::fs::read(&payload).ok().unwrap_or_default();
        let len = magic_bytes.len().min(4);
        let magic_hex = if len > 0 {
            format!("{:02X?}", &magic_bytes[..len])
        } else {
            "[]".to_string()
        };
        let ext = payload.extension().and_then(|e| e.to_str()).unwrap_or("none");
        anyhow::bail!(
            "unknown payload format for entry path: {} (ext: {}, magic: {})",
            payload.display(),
            ext,
            magic_hex
        );
    }

    let runner = get_runner_for_payload(&payload, &manifest);
    println!(
        "running {} v{} ({})",
        manifest.capsule.name, manifest.capsule.version, manifest.capsule.language
    );
    let result = runner.run(&manifest, &payload, &args)?;
    match result {
        ExecutionResult::Vm => {}
        ExecutionResult::Process(mut child) => {
            let status = child.wait()?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        ExecutionResult::None => {}
    }
    Ok(())
}

fn handle_check() -> anyhow::Result<()> {
    let (manifest, root) = load_manifest()?;
    println!(
        "checking {} v{}",
        manifest.capsule.name, manifest.capsule.version
    );
    let builder = PackageBuilder::new(manifest, root);
    builder.check()?;
    Ok(())
}

fn handle_pack(output: Option<PathBuf>) -> anyhow::Result<()> {
    let (manifest, root) = load_manifest()?;
    let output = output
        .unwrap_or_else(|| PathBuf::from(format!("{}.crush-pack", manifest.capsule.name)));
    pack(&root, &output)?;
    Ok(())
}

fn handle_unpack(pack: PathBuf, dir: Option<PathBuf>) -> anyhow::Result<()> {
    let name = pack
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("package");
    let dir = dir.unwrap_or_else(|| PathBuf::from(name));
    unpack(&pack, &dir)?;
    Ok(())
}

fn handle_keygen(dir: PathBuf) -> anyhow::Result<()> {
    generate_keys(&dir)?;
    Ok(())
}

fn handle_sign(package: PathBuf, key: PathBuf) -> anyhow::Result<()> {
    sign_package(&package, &key)?;
    Ok(())
}

fn handle_verify(package: PathBuf, key: PathBuf) -> anyhow::Result<()> {
    verify_package(&package, &key)?;
    Ok(())
}

fn handle_site(
    dir: PathBuf,
    name: String,
    version: String,
    entry: String,
    output: Option<PathBuf>,
    sign: Option<PathBuf>,
    did: Option<String>,
) -> anyhow::Result<()> {
    let output = output.unwrap_or_else(|| PathBuf::from(format!("{}.ecap", name)));
    let n = crush_pkg::site::write_site_capsule(
        &dir,
        &name,
        &version,
        &entry,
        &output,
        sign.as_deref(),
        did.as_deref(),
    )?;
    println!(
        "built static-site capsule {} ({} asset(s), entry={}{})",
        output.display(),
        n,
        entry,
        if sign.is_some() { ", signed" } else { "" }
    );
    Ok(())
}

fn handle_site_extract(capsule: PathBuf, dir: PathBuf) -> anyhow::Result<()> {
    let entry = crush_pkg::site::extract_site_capsule(&capsule, &dir)?;
    println!(
        "extracted {} -> {} (entry: {})",
        capsule.display(),
        dir.display(),
        entry
    );
    Ok(())
}

fn handle_show() -> anyhow::Result<()> {
    let (manifest, root) = load_manifest()?;
    println!("{}", crush_pkg::manifest::Manifest::to_toml_string(&manifest)?);
    println!("  (at {})", root.join("capsule.toml").display());
    Ok(())
}

/// `crush-pkg lint` handler — runs ONLY the dead-code detector
/// against the manifest (no full `build`). Emits findings via
/// [`emit_post_dispatch_lint`] and gates the process exit code
/// against strict-mode CI behavior: any finding under
/// `--message-format=strict` returns `Err` so dispatch routes
/// the failure to `CommandFailure::Lint` and `main()` exits 1
/// (the standard failure-path emit follows). Under non-strict,
/// findings are emitted as warnings but the process exits 0,
/// consistent with the post-build soft-emit behavior on
/// `crush-pkg build`.
///
/// Split into `handle_lint_with` (test seam: takes an explicit
/// manifest path AND an arbitrary writer) and `handle_lint`
/// (production wrapper: calls `find_manifest()` for path and
/// `std::io::stdout()` for the writer). Without the split, tests
/// would either depend on cwd-walk semantics or have to chdir
/// into a fixture dir + use `gag`-style stdout capture.
fn handle_lint_with(
    out: &mut impl Write,
    manifest_path: &Path,
    json_mode: bool,
    strict_mode: bool,
) -> anyhow::Result<()> {
    let count = emit_post_dispatch_lint(
        out,
        Some(manifest_path),
        json_mode,
        strict_mode,
    )?;
    if strict_mode && count > 0 {
        anyhow::bail!(
            "lint found {count} dead-code finding(s) in {} (fail under strict mode)",
            manifest_path.display()
        );
    }
    Ok(())
}

fn handle_lint(json_mode: bool, strict_mode: bool) -> anyhow::Result<()> {
    let manifest_path = find_manifest()?;
    let mut out = std::io::stdout();
    handle_lint_with(&mut out, manifest_path.as_path(), json_mode, strict_mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----------------------------------------------------------------
    // Per-binary code value lockdown.
    //
    // The seven-field wire SHAPE is locked in
    // `crush_diagnostics/tests/wire_format.rs` (a single canonical
    // source of truth across `xtask`, `crush-vm`,
    // `crush-installer`, `crush-pkg`). The per-binary CODE-VALUES
    // stay here so a refactor that renames one in crush-pkg without
    // updating the canonical literal list fails synchronously.
    // ----------------------------------------------------------------

    #[test]
    fn diag_record_inline_codes_resolve_to_expected_strings() {
        assert_eq!(CODE_NEW, "E-NEW");
        assert_eq!(CODE_BUILDER, "E-BUILDER");
        assert_eq!(CODE_RUN, "E-RUN");
        assert_eq!(CODE_SIGN, "E-SIGN");
        assert_eq!(CODE_SITE, "E-SITE");
        assert_eq!(CODE_MANIFEST, "E-MANIFEST");
        assert_eq!(CODE_LINT, "E-LINT");
    }

    // ----------------------------------------------------------------
    // Per-binary CommandFailure enum → wire-code routing.
    //
    // Not a wire-format concern (the shape is locked in
    // wire_format.rs); this is the per-binary routing table that
    // pairs each domain-handler fn with the right wire code. Keep
    // here so adding a new domain (e.g. `Lint` for `crush-pkg lint`)
    // forces an update to the routing table AND the enum, fails
    // sync if either is missed.
    // ----------------------------------------------------------------

    #[test]
    fn command_failure_code_and_message_routes_per_domain() {
        let cases: [(CommandFailure, &str, &str); 7] = [
            (CommandFailure::New("x".into()), CODE_NEW, "x"),
            (CommandFailure::Builder("y".into()), CODE_BUILDER, "y"),
            (CommandFailure::Run("z".into()), CODE_RUN, "z"),
            (CommandFailure::Sign("a".into()), CODE_SIGN, "a"),
            (CommandFailure::Site("b".into()), CODE_SITE, "b"),
            (CommandFailure::Manifest("c".into()), CODE_MANIFEST, "c"),
            (CommandFailure::Lint("d".into()), CODE_LINT, "d"),
        ];
        for (failure, want_code, want_msg) in cases {
            let (code, msg) = failure.code_and_message();
            assert_eq!(code, want_code, "code mismatch for {want_code}");
            assert_eq!(msg, want_msg, "message mismatch for {want_code}");
        }
    }

    // ----------------------------------------------------------------
    // import-smoke — confirm the canonical re-export resolves
    // cleanly. Full wire-format lockdown is in
    // `crush_diagnostics/tests/wire_format.rs`.
    // ----------------------------------------------------------------

    #[test]
    fn diagnostics_import_resolves() {
        // Smoke: the imported `DiagRecord` shape accepts the
        // canonical seven-field wire codes used across the
        // quarantine sites; a future contributor who re-routes the
        // import to a wrong module fails this test synchronously.
        let rec = DiagRecord {
            code: CODE_BUILDER,
            level: "error",
            file: None,
            line: None,
            col: None,
            message: "smoke",
            hint: None,
        };
        // Sanity: every field is set; this also exercises that
        // `DiagRecord` is `pub` from `crush_diagnostics`.
        assert_eq!(rec.code, "E-BUILDER");
        assert_eq!(rec.level, "error");
        assert_eq!(rec.message, "smoke");
    }

    // ----------------------------------------------------------------
    // Strict-mode lockdown (third value of
    // `--message-format=FORMAT`).
    //
    // Coverage matrix:
    //   A. clap-level smoke: third value accepted; existing
    //      text/json values + default still work; unknown values
    //      rejected (no silent free-form acceptance).
    //   B. Pure-helper unit: passthrough symmetry (only `note`
    //      lifts under strict).
    //   C. Four-tuple pin: code/level/file/line survive the
    //      canonical `diag_line` wire shape AND the strict-mode
    //      downgrade. The "dead-code in `capsule.toml`" example
    //      class is the canonical reader-side contract that
    //      editor consumers and CI gates rely on.
    //   D. emit integration, strict: `Vec<u8>` capture confirms
    //      the wire shape survives end-to-end.
    //   E. emit integration, non-strict: emit with strict_mode=false
    //      preserves `level: "note"` (D + E together prove the
    //      downgrade logic is the ONLY thing that changes across
    //      modes).
    // ----------------------------------------------------------------

    /// A. clap-level surface: every `MessageFormat::value_variants()`
    /// entry parses AND `foo` is rejected. Iterating via clap's
    /// `ValueEnum::value_variants()` const slice means the test is
    /// **structurally in lockstep with the `MessageFormat` enum** —
    /// adding a new variant (e.g. `CompactJson`) is a one-line edit
    /// (`MessageFormat` enum only) and the test picks it up without
    /// manual row addition.
    ///
    /// Round-trip uses clap's native `to_possible_value()` API to
    /// extract the kebab-lowercase name clap actually accepts on
    /// the command line. Re-implementing kebab-lowercasing via
    /// `variant.to_string().to_lowercase()` would break for future
    /// multi-word variants like `CompactJson` (clap expects
    /// `"compact-json"`, but `to_lowercase()` produces
    /// `"compactjson"`). Mirrors the iterate-the-surface pattern in
    /// [`strict_downgrade_does_not_change_non_note_levels`] test B
    /// below.
    #[test]
    fn cli_message_format_accepts_third_strict_option() {
        use clap::Parser;
        // Iterate clap's auto-generated variant slice — every
        // enum entry round-trips through `--message-format=<kebab>`.
        // Structural lockstep: the table is no longer hand-maintained.
        for &variant in MessageFormat::value_variants() {
            // Use clap's native `to_possible_value` to get the
            // kebab-lowercase name the derive macro produces
            // (single-source-of-truth: clap, not our test code).
            let arg = format!(
                "--message-format={}",
                variant
                    .to_possible_value()
                    .expect("ValueEnum derive always produces a PossibleValue")
                    .get_name(),
            );
            let argv: [&str; 3] = ["crush-pkg", "build", arg.as_str()];
            let cli = Cli::try_parse_from(argv)
                .unwrap_or_else(|e| panic!("parse {argv:?} failed: {e}"));
            assert_eq!(
                cli.message_format,
                Some(variant),
                "argv {argv:?} should parse to Some({variant:?})"
            );
        }
        // Default (flag absent) parses to None — text-mode stays
        // the implicit default for callers that don't pass
        // `--message-format=...`.
        let cli = Cli::try_parse_from(["crush-pkg", "build"])
            .expect("absent flag should parse to message_format=None");
        assert_eq!(cli.message_format, None);
        // Unknown value rejected — clap `ValueEnum` doesn't
        // silently accept free-form (the prior `Option<String>`
        // shape did, which would have missed both `strict` and
        // `STREAM` typos — the latter would have been silently
        // treated as text-mode under the old shape).
        assert!(
            Cli::try_parse_from(["crush-pkg", "build", "--message-format=foo"]).is_err(),
            "unknown value foo must be rejected by clap ValueEnum"
        );
    }

    /// B. Pure-helper unit: only `note` lifts under strict mode;
    /// everything else (including `note` under non-strict) passes
    /// through unchanged.
    #[test]
    fn strict_downgrade_does_not_change_non_note_levels() {
        // Strict mode + non-note level: passthrough.
        assert_eq!(strict_downgrade("error", true), "error");
        assert_eq!(strict_downgrade("warning", true), "warning");
        assert_eq!(strict_downgrade("info", true), "info");
        // Non-strict mode: notes pass through unchanged too —
        // strict is the ONLY trigger that lifts note → error.
        assert_eq!(strict_downgrade("note", false), "note");
        assert_eq!(strict_downgrade("error", false), "error");
        // The strict-mode CI-gate kernel: note lifts to error.
        assert_eq!(strict_downgrade("note", true), "error");
    }

    /// C. End-to-end four-tuple pin: code/level/file/line survive
    /// both the canonical `diag_line` wire shape AND the
    /// strict-mode downgrade. The "dead-code in `capsule.toml`"
    /// example class — exactly what a future `crush-pkg build`
    /// lint will emit at `level: "note"` — is the canonical
    /// reader-side contract that editor consumers and CI gates
    /// rely on.
    #[test]
    fn strict_mode_pins_warning_class_four_tuple_in_diag_record() {
        // struct form (rather than function-form `diag_line_from`)
        // because the function form hardcodes `line: None,
        // col: None`; the four-tuple pin REQUIRES explicit line,
        // so the struct form is the right tool here.
        // Bind the downgraded level to a local first so the
        // struct's `level` borrow is valid through the
        // subsequent `diag_line(&rec)` call (otherwise the
        // temporary Cow would be dropped at the end of the
        // struct expression).
        let downgraded_level = strict_downgrade("note", true);
        let rec = DiagRecord {
            code: CODE_BUILDER,
            level: downgraded_level,
            file: Some("capsule.toml"),
            line: Some(42),
            col: None,
            message: "dead-code: unused capsule field `alpha`",
            hint: Some("remove or rename `alpha`"),
        };
        let line = diag_line(&rec);
        // The canonical encoding preserves every field exactly.
        let v: serde_json::Value = serde_json::from_str(line.trim_end())
            .expect("four-tuple record must round-trip via serde");
        // Four-tuple pin — the read-side contract:
        assert_eq!(v["code"], "E-BUILDER");
        assert_eq!(
            v["level"], "error",
            "strict mode MUST downgrade level=note → level=error (got: {:?})",
            v["level"]
        );
        assert_eq!(v["file"], "capsule.toml");
        assert_eq!(v["line"], 42, "line is JSON number, not string");
    }

    /// D. End-to-end emit (strict): write-through
    /// `emit_diag(strict=true)` to a `Vec<u8>`, parse back, level
    /// lifts note → error.
    #[test]
    fn emit_diag_strict_mode_lifts_note_to_error() {
        let mut out = Vec::<u8>::new();
        emit_diag(
            &mut out,
            CODE_BUILDER,
            "note",
            "synthetic-warn",
            Some("capsule.toml"),
            None,  // line
            Some("h"),
            true, // strict_mode
        )
        .expect("emit_diag should write to a Vec<u8>");
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.ends_with('\n'), "NDJSON record must end with newline");
        let v: serde_json::Value = serde_json::from_str(s.trim_end()).unwrap();
        assert_eq!(v["code"], "E-BUILDER");
        assert_eq!(
            v["level"], "error",
            "strict mode MUST lift note → error (got: {:?})",
            v["level"]
        );
        assert_eq!(v["message"], "synthetic-warn");
    }

    /// E. Non-strict symmetry: emit under non-strict mode preserves
    /// `level: "note"` (does NOT downgrade). Pairs with D so the
    /// downgrade logic is the ONLY thing that changes across the
    /// two modes.
    #[test]
    fn emit_diag_non_strict_mode_passes_note_through() {
        let mut out = Vec::<u8>::new();
        emit_diag(
            &mut out,
            CODE_BUILDER,
            "note",
            "info-only",
            Some("capsule.toml"),
            None,  // line
            None,  // hint
            false, // strict_mode=false
        )
        .expect("emit_diag");
        let s = std::str::from_utf8(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim_end()).unwrap();
        assert_eq!(
            v["level"], "note",
            "non-strict mode must NOT downgrade (got: {:?})",
            v["level"]
        );
    }

    // ----------------------------------------------------------------
    // Real-emit-site lockdown for `capsule.toml` dead-code detection
    //
    // Closes the gap between the four-tuple pin test (C above) and
    // the actual lint the strict-mode CI gate is designed to
    // enforce: `emit_post_dispatch_lint` is the production call
    // site that routes findings through `emit_diag` so strict
    // mode can lift note → error at wire time.
    //
    // Coverage matrix:
    //   F. Post-dispatch lint under JSON+non-strict emits one
    //      canonical NDJSON record per finding, with file/line/
    //      message/hint pinned via the detector's own output.
    //   G. Post-dispatch lint under JSON+strict lifts each note
    //      record to level=error (the CI-gate behavior the four-
    //      tuple pin test in C asserts is reachable from a real
    //      detector, not a hand-crafted synthetic DiagRecord).
    // ----------------------------------------------------------------

    /// Helper: write a `capsule.toml` fixture into a tempdir and
    /// return BOTH the [`tempfile::TempDir`] (the lifecycle owner
    /// of the on-disk directory) AND the manifest path. Returning
    /// only the path would mean the [`tempfile::TempDir`] is
    /// dropped when this helper returns, deleting the directory
    /// and silently making the subsequent `read_to_string` fail
    /// (the `Err(_) => return Ok(0)` arm in
    /// `emit_post_dispatch_lint` would mask it as "no findings").
    /// Tests must bind both returns so the TempDir outlives the
    /// read.
    fn write_capsule_with_placeholder(
        content: &str,
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let manifest_path = dir.path().join("capsule.toml");
        std::fs::write(&manifest_path, content).expect("write fixture capsule.toml");
        (dir, manifest_path)
    }

    /// F. JSON mode + non-strict: emit canonical note record for
    /// every dead-code finding. Parses back from `Vec<u8>` so the
    /// wire shape (code/level/file/line/message/hint) is verified
    /// end-to-end — not just synthesized — from the detector's
    /// real output.
    #[test]
    fn post_dispatch_lint_emits_canonical_note_record_for_alpha_placeholder() {
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nTEMP_ENDPOINT = \"alpha\"\n",
        );
        let mut out = Vec::<u8>::new();
        let count = emit_post_dispatch_lint(
            &mut out,
            Some(&manifest_path),
            /* json_mode */ true,
            /* strict_mode */ false,
        )
        .expect("emit_post_dispatch_lint should write cleanly to Vec<u8>");
        assert_eq!(count, 1, "exactly one finding expected");
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.ends_with('\n'), "NDJSON record must end with newline");
        let v: serde_json::Value =
            serde_json::from_str(s.trim_end()).expect("record must round-trip via serde");
        // Canonical four-tuple pin (mirror of test C's lockdown
        // but loaded from the REAL detector's output):
        assert_eq!(
            v["code"], "E-BUILDER",
            "real emit must use CODE_BUILDER (got: {:?})",
            v["code"]
        );
        assert_eq!(
            v["level"], "note",
            "non-strict mode keeps level=note (got: {:?})",
            v["level"]
        );
        assert_eq!(v["file"], "capsule.toml");
        // TEMP_ENDPOINT is on line 5 of the 5-line fixture.
        assert_eq!(
            v["line"], 5,
            "line number should match fixture position (got: {})",
            v["line"]
        );
        // Message includes the key name — the user-facing
        // signal that tells the developer WHERE the dead code
        // is in their capsule.toml.
        assert!(
            v["message"].as_str().unwrap().contains("TEMP_ENDPOINT"),
            "message should name the offending key, got: {:?}",
            v["message"]
        );
        // Message comes verbatim from the rule's emit-shape (the
        // `PlaceholderValue` arm of the dispatch loop); strip-local
        // prefixes like `dead-code:` belong to text-mode rendering,
        // not the JSON message slot.
        assert!(
            v["message"]
                .as_str()
                .unwrap()
                .contains("placeholder value"),
            "message should describe the rule semantic (placeholder value), got: {:?}",
            v["message"]
        );
        // Hint slot: actionable guidance the editor displays in
        // the gutter / "do this next" affordance — comes verbatim
        // from the rule.
        assert!(
            v["hint"].as_str().unwrap().contains("TEMP_ENDPOINT"),
            "hint should reference the offending key, got: {:?}",
            v["hint"]
        );
        // Hint must NOT prefix `dead-code:` — that prefix belongs
        // to the text-mode renderer, not the canonical wire.
        assert!(
            !v["hint"].as_str().unwrap().starts_with("dead-code:"),
            "hint must NOT carry text-mode prefix, got: {:?}",
            v["hint"]
        );
    }

    /// G. JSON mode + strict: each note record is lifted to
    /// level=error via `strict_downgrade` (same helper the rest
    /// of the codebase uses). Pairs with F so the ONLY
    /// difference between the two modes is the note→error lift
    /// — the detection logic itself is shared.
    #[test]
    fn post_dispatch_lint_strict_mode_lifts_note_to_error_for_alpha_placeholder() {
        // Use `"DEBUG"` (uppercase) so the placeholder match
        // succeeds — `"debug"` (lowercase) is intentionally
        // NOT in `DEAD_CODE_PLACEHOLDERS` because `LOG_LEVEL =
        // "debug"` is a legitimate production env value and
        // matching it would be a false-positive catastrophe.
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nDEBUG_FLAG = \"DEBUG\"\n",
        );
        let mut out = Vec::<u8>::new();
        let count = emit_post_dispatch_lint(
            &mut out,
            Some(&manifest_path),
            /* json_mode */ true,
            /* strict_mode */ true,
        )
        .expect("emit_post_dispatch_lint should write cleanly to Vec<u8>");
        assert_eq!(count, 1);
        let s = std::str::from_utf8(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim_end()).unwrap();
        // Strict-mode CI-gate kernel: real-detector output lifted
        // note → error via the shared `strict_downgrade` helper.
        // Confirms the four-tuple pin test (C) is reachable from a
        // production call site, not a synthetic DiagRecord.
        assert_eq!(
            v["level"], "error",
            "strict mode MUST lift real-detector note → error (got: {:?})",
            v["level"]
        );
        // Everything else survives: code, file, line. Only
        // `level` changes — strict-mode downgrade is the ONLY
        // mutation across the two modes.
        assert_eq!(v["code"], "E-BUILDER");
        assert_eq!(v["file"], "capsule.toml");
        assert_eq!(v["line"], 5, "DEBUG_FLAG is on line 5 of the fixture");
        assert!(
            v["message"].as_str().unwrap().contains("DEBUG_FLAG"),
            "message should name the offending key, got: {:?}",
            v["message"]
        );
    }

    /// H. Text mode: emit human-readable single-line stdout note
    /// (`note: capsule.toml:N: dead-code: ...`) — same source-of-
    /// truth detection, different visual shape. Pairs with F and
    /// G so all three modes (text / json / strict) share one
    /// detector and one wire shape semantics.
    #[test]
    fn post_dispatch_lint_text_mode_emits_human_readable_per_finding() {
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nTEMP_ENDPOINT = \"alpha\"\n",
        );
        let mut out = Vec::<u8>::new();
        let count = emit_post_dispatch_lint(
            &mut out,
            Some(&manifest_path),
            /* json_mode */ false,
            /* strict_mode */ false,
        )
        .expect("emit_post_dispatch_lint should write cleanly to Vec<u8>");
        assert_eq!(count, 1);
        let s = std::str::from_utf8(&out).unwrap();
        // Text-mode emits the canonical `note:` prefix + the
        // rule's message verbatim (no `key` re-substitution). TEMP_ENDPOINT
        // is the key name; placeholder value `alpha` is what the rule
        // detected on that line.
        assert!(
            s.starts_with(
                "note: capsule.toml:5: dead-code: placeholder value `alpha` on [env] key `TEMP_ENDPOINT`\n"
            ),
            "text-mode emit must begin with the canonical note-prefix + rule message, got: {s:?}"
        );
    }

    /// I. No findings: emit returns 0 and writes nothing in any
    /// mode (text / json / strict). Confirms the post-dispatch
    /// path is a no-op when the manifest is clean — no spurious
    /// empty records, no spurious empty stdout noise.
    #[test]
    fn post_dispatch_lint_emits_zero_records_when_no_findings() {
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nLOG_LEVEL = \"debug\"\n",
        );
        let mut out = Vec::<u8>::new();
        let count = emit_post_dispatch_lint(
            &mut out,
            Some(&manifest_path),
            /* json_mode */ true,
            /* strict_mode */ true,
        )
        .expect("emit_post_dispatch_lint must not error on clean manifest");
        assert_eq!(count, 0, "clean manifest → zero findings");
        assert!(
            out.is_empty(),
            "no findings → no bytes written (got: {:?})",
            std::str::from_utf8(&out).unwrap_or("<non-utf8>")
        );
    }

    // ----------------------------------------------------------------
    // `crush-pkg lint` subcommand lockdown
    //
    // Coverage matrix:
    //   J. clap-level: `Cli::try_parse_from(["crush-pkg", "lint"])`
    //      parses to `Commands::Lint {}`. Pin so a contributor who
    //      renames the subcommand breaks the dispatcher.
    //   K. JSON mode + non-strict: `handle_lint_with` returns
    //      Ok(()) and emits the canonical NDJSON note record via
    //      the underlying `emit_post_dispatch_lint` helper.
    //   L. JSON mode + strict: under `--message-format=strict`,
    //      `handle_lint_with` bails with the gate-trip message
    //      (which routes through `CommandFailure::Lint` →
    //      `E-LINT` at the failure-path emit).
    //   M. Clean manifest + any mode: `handle_lint_with` is a
    //      no-op (zero findings, zero bytes, Ok).
    // ----------------------------------------------------------------

    /// J. clap-level: `crush-pkg lint` parses to `Commands::Lint {}`.
    #[test]
    fn cli_lint_subcommand_parses_to_commands_lint_variant() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["crush-pkg", "lint"])
            .expect("crush-pkg lint must parse without args");
        // Pattern-match on the structure: the only field-less
        // variant is `Lint {}`. Confirms `Lint` is selected and
        // the enum stays field-less.
        match &cli.command {
            Commands::Lint {} => { /* correct variant */ }
            other => panic!(
                "expected `Commands::Lint {{}}`, got a different variant: {:?}",
                other
            ),
        }
        // `lint` doesn't take the global `--message-format`
        // position sensitivity assumption from `build` — verify
        // both pre- and post-subcommand positions are accepted
        // (same `global = true` behavior as build/check).
        let pre = Cli::try_parse_from([
            "crush-pkg",
            "--message-format=json",
            "lint",
        ])
        .expect("--message-format before subcommand must parse");
        let post = Cli::try_parse_from([
            "crush-pkg",
            "lint",
            "--message-format=strict",
        ])
        .expect("--message-format after subcommand must parse");
        assert!(matches!(pre.command, Commands::Lint { .. }));
        assert!(matches!(post.command, Commands::Lint { .. }));
    }

    /// K. `handle_lint_with` under JSON+non-strict: emits the
    /// canonical note record AND returns Ok(()) (strict-mode gate
    /// didn't trip). Verifies the recursive call into
    /// `emit_post_dispatch_lint` survives the wrapper.
    #[test]
    fn handle_lint_with_emits_ndjson_record_per_finding_json_mode() {
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nTEMP_ENDPOINT = \"alpha\"\n",
        );
        let mut out = Vec::<u8>::new();
        // Discard the Ok(()): the `expect(...)` already asserts
        // non-bail; we keep the call flow here so lint failure
        // paths still get exercised under the `let _ =` form,
        // but the return value itself isn't otherwise useful.
        let _ = handle_lint_with(
            &mut out,
            &manifest_path,
            /* json_mode */ true,
            /* strict_mode */ false,
        )
        .expect("non-strict mode must NOT bail on findings");
        let s = std::str::from_utf8(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim_end())
            .expect("NDJSON record must round-trip via serde");
        // Per-finding wire shape mirrors `crush-pkg build`'s
        // post-dispatch emit (same helper, same canonical shape):
        assert_eq!(v["code"], "E-BUILDER");
        assert_eq!(v["level"], "note");
        assert_eq!(v["file"], "capsule.toml");
        assert_eq!(v["line"], 5);
        assert!(v["message"].as_str().unwrap().contains("TEMP_ENDPOINT"));
    }

    /// L. Strict-mode CI gate: `handle_lint_with` under strict
    /// mode with findings bails (Err). This is what triggers
    /// dispatch's `CommandFailure::Lint` arm and `main()`'s
    /// exit-1 failure-path emit.
    #[test]
    fn handle_lint_with_strict_mode_bails_when_findings_present() {
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nDEBUG_FLAG = \"DEBUG\"\n",
        );
        let mut out = Vec::<u8>::new();
        let err = handle_lint_with(
            &mut out,
            &manifest_path,
            /* json_mode */ true,
            /* strict_mode */ true,
        )
        .expect_err(
            "strict mode MUST bail when findings exist (CI gate behavior)",
        );
        let msg = format!("{err:#}");
        // The bail message carries the count + path so dispatch's
        // failure-path emit (which surfaces `{e:#}` as the
        // `message` field) gives the editor/CI consumer
        // actionable context.
        assert!(
            msg.contains("1 dead-code finding"),
            "bail message should report the count (got: {msg:?})"
        );
        assert!(
            msg.contains("strict mode"),
            "bail message should call out strict-mode (got: {msg:?})"
        );
    }

    /// M. Clean manifest: zero findings, zero bytes, Ok(()).
    /// Pairs with K + L so the matrix (clean/dirty ×
    /// strict/non-strict) has its full diagonal pinned.
    #[test]
    fn handle_lint_with_clean_capsule_returns_zero_findings_ok() {
        let (_dir, manifest_path) = write_capsule_with_placeholder(
            "[capsule]\nname = \"my-pkg\"\n\n[env]\nLOG_LEVEL = \"debug\"\n",
        );
        let mut out = Vec::<u8>::new();
        // Strict mode + clean manifest: gate does NOT trip (no
        // findings), so Ok(()) even under strict. This is the
        // CI-gate invariant: clean ⇒ exit 0 under any strictness.
        handle_lint_with(
            &mut out,
            &manifest_path,
            /* json_mode */ true,
            /* strict_mode */ true,
        )
        .expect("strict mode + clean manifest must remain Ok");
        assert!(
            out.is_empty(),
            "clean capsule must NOT emit any bytes (got: {:?})",
            std::str::from_utf8(&out).unwrap_or("<non-utf8>")
        );
    }

    // ----------------------------------------------------------------
    // END-TO-END CROSS-REF PIN
    // ----------------------------------------------------------------
    // Closes the round-1 reviewer-flagged coverage gap: exercise the
    // full entry-aware wiring —
    //   `Manifest::from_str` →
    //   `manifest_path.parent()` →
    //   `parent().join(&manifest.capsule.entry)` →
    //   `builder::scan_entry_file_references` →
    //   `lint_capsule_toml_with_entry`
    // — through the public entry point `emit_post_dispatch_lint`,
    // not just the unit-level dispatcher. The earlier unit tests
    // exercise the dispatcher shape in isolation; this test pins
    // that the on-disk glue (manifest reparsing + entry join +
    // cross-ref scanner) actually threads through.
    //
    // Strong-assertion strategy: bundle a `[env]` placeholder
    // (which MUST fire) with a `[[dependencies]]` entry whose name
    // IS mentioned by the on-disk entry file (which MUST NOT fire,
    // because the cross-ref satisfied it).
    //
    // Two independent failure modes for one regression:
    //   - `count`: deviates from `1` if `scan_entry_file_references`
    //     returned `None` (manifest unparseable / no `entry` field /
    //     entry file unreadable) or if the dispatcher cross-tab
    //     mis-routed the dep row.
    //   - `message` substring: if the cross-ref succeeded but the
    //     dep rule still fired with a different shape, the negative
    //     `alpha-dep` containment check trips.
    #[test]
    fn handle_lint_with_referenced_dep_suppresses_finding_end_to_end() {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let manifest_path = dir.path().join("capsule.toml");
        // Minimum valid `[capsule]` — `name` + `entry` are required.
        // `language` is included explicitly to defeat the obsolete-key
        // auto-migrate shadowing (`capsule_type` would add a SECOND
        // finding and mask the assertion). Note: `language = "crush"`
        // is NOT scanned by the obsolete-key rule (that rule keys on
        // `capsule_type`); we set `language` so `Manifest::from_str`
        // does not trip any required-field check.
        std::fs::write(
            &manifest_path,
            "\
[capsule]
name = \"test-pkg-cross-ref-pin\"
language = \"crush\"
entry = \"main.crush\"

[env]
TEMP = \"TODO\"

[[dependencies]]
name = \"alpha-dep\"
",
        )
        .expect("write manifest");
        // On-disk entry file co-located with the manifest. The bare
        // `alpha-dep` identifier post-`#`-strip satisfies the cross-ref
        // so the dep MUST NOT be flagged as unreferenced. (A comment-
        // only mention like `# alpha-dep` would NOT satisfy it —
        // comment strips are now part of the dispatch contract.)
        std::fs::write(
            dir.path().join("main.crush"),
            "import alpha-dep\n",
        )
        .expect("write entry file");

        let mut out = Vec::<u8>::new();
        let count = emit_post_dispatch_lint(
            &mut out,
            Some(&manifest_path),
            /* json_mode */ true,
            /* strict_mode */ false,
        )
        .expect("emit_post_dispatch_lint must not error on cross-ref'd manifest");
        assert_eq!(
            count, 1,
            "expected 1 finding (env placeholder); dep cross-ref should suppress \
             the dep finding. If count = 2, the entry-aware wiring regressed: \
             either `Manifest::from_str` failed, `parent().join(&entry)` resolved \
             to a path `scan_entry_file_references` couldn't read, or the dispatcher \
             fired the unreferenced-dep rule despite the entry mentioning `alpha-dep`."
        );
        let s = std::str::from_utf8(&out).expect("findings must be UTF-8");
        assert!(s.ends_with('\n'), "NDJSON record must end with newline");
        let v: serde_json::Value =
            serde_json::from_str(s.trim_end()).expect("record must round-trip via serde");
        // Wire-shape pins — the canonical 4-tuple lockdown from
        // `crush_diagnostics::tests::wire_format` is preserved.
        assert_eq!(
            v["code"], "E-BUILDER",
            "wire code must remain the canonical lint code"
        );
        assert_eq!(
            v["level"], "note",
            "wire level must remain `note` for non-strict lint"
        );
        // Line-number pin: the env `[env]` block lives at lines 7-8
        // in the fixture (line 0-4 is the file head, line 5 is the
        // `[env]` header, line 6 is the BOM-ish blank, line 7 is the
        // `TEMP = "TODO"` row). Asserting `>= 7` also pins that the
        // finding came from the env section, NOT the deps section
        // — catches a regression where the dispatcher attributed the
        // finding to the wrong section while still emitting exactly
        // one record.
        let line = v["line"]
            .as_u64()
            .expect("line must be a non-negative integer");
        // Bracket the env block: the `[env]` block lives at lines 6-7
        // in the fixture (line 6 = `[env]` header, line 7 = the
        // `TEMP = \"TODO\"` row); `[[dependencies]]` starts at line 9.
        // A bracket (not bare `>=`) survives future blank-line tweaks
        // gracefully. Also pins that the finding came from the ENV
        // section, NOT the deps section — catches section-attribution
        // regressions that pass a bare upper/lower bound.
        assert!(
            line >= 7 && line < 9,
            "finding line must point into the [env] block (7 ≤ line < 9 in the fixture); got line = {line}"
        );
        let msg = v["message"]
            .as_str()
            .expect("message must be a string");
        assert!(
            msg.contains("TEMP"),
            "env placeholder finding must name the env key `TEMP` (got: {msg:?})"
        );
        // Negative pin: the unreferenced-dep finding would surface
        // its name here. Suppressed ⇒ cross-ref path is verified.
        assert!(
            !msg.contains("alpha-dep"),
            "dep cross-ref should suppress the `alpha-dep` finding (got: {msg:?})"
        );
        // Second negative pin: the unreferenced-dep rule's `hint`
        // ALSO mentions the dep name
        // (e.g. "remove `alpha-dep` from [dependencies]..."). Cover
        // `hint` so we catch a regression where the rule fires with
        // a valid message but an empty/leaking hint.
        // Canonical wire-shape pins from `crush_diagnostics::tests::wire_format`:
        // source-level findings attribute `file = capsule.toml` and
        // `col = None` (manifest has no meaningful column).
        assert_eq!(
            v["file"], "capsule.toml",
            "wire file must point at the in-fixture manifest"
        );
        assert_eq!(
            v.get("col"),
            Some(&serde_json::Value::Null),
            "wire col must be present + null for source-level findings (got: {:?})",
            v.get("col")
        );
        let hint = v["hint"]
            .as_str()
            .expect("hint must be a string");
        assert!(
            !hint.contains("alpha-dep"),
            "dep cross-ref should suppress the `alpha-dep` hint leak (got: {hint:?})"
        );
    }

    // ----------------------------------------------------------------
    // FEDPATH SNAPSHOT — byte-exact NDJSON for ALL 3 rules in one dispatch
    // ----------------------------------------------------------------
    // Closes the byte-drift gap that the prior lockdown tests leave
    // implicit: each test pins one rule family in isolation. A
    // future contributor who edits the dispatch loop (line order /
    // section priority / rule order) or the wire shape
    // (DiagRecord field order, `Option<&str>` rendering of `None`,
    // `Option<u32>` rendering of `Some(N)`) could pass every
    // individual test while silently shifting what multi-rule
    // fedpaths emit — the exact bytes editors and watch-exec
    // front-ends see on stdout.
    //
    // This test fires ALL THREE current dead-code rule families in
    // a single fixture (`PlaceholderValue` env + `ObsoleteKey`
    // `capsule_type` + `UnreferencedDependency` bare dep) and pins
    // the resulting NDJSON stream as a single byte-exact string.
    // Future drift ⇒ a single failed `assert_eq!` with a precise
    // `actual != expected` diff that links the byte position to
    // the rule that drifted.
    //
    // Canonical pipeline exercised:
    //   1. `Manifest::from_str` accepts the auto-migrating
    //      `capsule_type = "Crush"` + `entry = "main.crush"`
    //      fixture (manifest.rs auto-migrate at lines 327-356).
    //   2. `parent().join("main.crush")` resolves to a real on-
    //      disk file (`fn main() { io.print("hello") }`).
    //   3. `scan_entry_file_references` returns a `Some(refs)`
    //      set that does NOT contain `beta-dep` (the fixture's
    //      entry file mentions only `fn`, `main`, `io`, `print`,
    //      `hello` — none match `beta-dep`).
    //   4. `lint_capsule_toml_with_entry` iterates the TOML
    //      lines in file order, pushes ObsoleteKey @ line 3 +
    //      PlaceholderValue @ line 7 from the main dispatch
    //      loop, then UnreferencedDependency @ line 10 from the
    //      post-loop cross-reference pass.
    //   5. `emit_post_dispatch_lint` emits each finding via
    //      `emit_diag` → `DiagRecord` → `serde_json::to_string`
    //      in struct-declaration order
    //      (`code, level, file, line, col, message, hint`).
    #[test]
    fn handle_lint_with_byte_exact_three_rule_fedpath() {
        let dir = tempfile::tempdir().expect("tempdir creation");
        let manifest_path = dir.path().join("capsule.toml");
        // Fixture layout (line numbering is 1-based):
        //   1  [capsule]
        //   2  name = "three-rule-fedpath"
        //   3  capsule_type = "Crush"        ← ObsoleteKey finding
        //   4  entry = "main.crush"
        //   5  (blank)
        //   6  [env]
        //   7  TEMP = "TODO"                 ← PlaceholderValue finding
        //   8  (blank)
        //   9  [[dependencies]]
        //  10  name = "beta-dep"             ← UnreferencedDependency finding
        //
        // `capsule_type = "Crush"` exists ONLY to trigger the
        // obsolete-key rule — it is auto-migrated to `language`
        // by `Manifest::from_str`, but the lint dispatcher scans
        // the raw TOML so the obsolete finding still fires.
        std::fs::write(
            &manifest_path,
            "\
[capsule]
name = \"three-rule-fedpath\"
capsule_type = \"Crush\"
entry = \"main.crush\"

[env]
TEMP = \"TODO\"

[[dependencies]]
name = \"beta-dep\"
",
        )
        .expect("write manifest");
        // Entry file deliberately does NOT mention `beta-dep` →
        // the UnreferencedDependency rule fires on the dep row.
        // Token split: scanner surfaces `fn`, `main`, `io`,
        // `print`, `hello` — none match `beta-dep`.
        std::fs::write(
            dir.path().join("main.crush"),
            "fn main() {\n    io.print(\"hello\")\n}\n",
        )
        .expect("write entry file");

        let mut out = Vec::<u8>::new();
        let count = emit_post_dispatch_lint(
            &mut out,
            Some(&manifest_path),
            /* json_mode */ true,
            /* strict_mode */ false,
        )
        .expect("emit_post_dispatch_lint must accept a 3-rule fixture");
        assert_eq!(
            count, 3,
            "ticket spec: ONE fixture must fire ALL 3 current rule families \
             (PlaceholderValue env + ObsoleteKey capsule_type + \
             UnreferencedDependency bare dep)"
        );

        // ─── BYTE-EXACT NDJSON PIN ────────────────────────────
        // Dispatcher emits findings in TOML-line order:
        //   line 3 (ObsoleteKey)
        //   → line 7 (PlaceholderValue)
        //   → line 10 (UnreferencedDependency, post-loop cross-ref).
        // Each line is the `serde_json::to_string` of a
        // `DiagRecord` in STRUCT-DECLARATION order
        // (`code, level, file, line, col, message, hint`):
        //   - `code`: "E-BUILDER" (canonical lint code)
        //   - `level`: "note" (non-strict; strict would lift to "error")
        //   - `file`: "capsule.toml" (source-path pin)
        //   - `line`: N (1-based, TOML convention)
        //   - `col`: null (source-level findings have no column)
        //   - `message`: rule-emit shape verbatim
        //   - `hint`: rule remediation text verbatim (no prefix)
        // A future edit to ANY field order, ANY `Option<T>`
        // rendering (`null` vs omit), ANY dispatch ordering,
        // ANY rule message/hint text, or ANY byte of the
        // structural shape fails this test with a precise diff.
        let expected = "\
{\"code\":\"E-BUILDER\",\"level\":\"note\",\"file\":\"capsule.toml\",\"line\":3,\"col\":null,\"message\":\"obsolete key `capsule_type` on [capsule]\",\"hint\":\"rename `capsule_type` to `language` (or remove the field)\"}
{\"code\":\"E-BUILDER\",\"level\":\"note\",\"file\":\"capsule.toml\",\"line\":7,\"col\":null,\"message\":\"placeholder value `TODO` on [env] key `TEMP`\",\"hint\":\"on key `TEMP`: replace `TODO` with a real value, or remove `TEMP`\"}
{\"code\":\"E-BUILDER\",\"level\":\"note\",\"file\":\"capsule.toml\",\"line\":10,\"col\":null,\"message\":\"dependency `beta-dep` declared in [dependencies] but not referenced by entry file\",\"hint\":\"remove `beta-dep` from [dependencies] or reference it in the entry file\"}
";
        let actual =
            std::str::from_utf8(&out).expect("NDJSON output must be UTF-8");
        assert_eq!(
            actual, expected,
            "byte-exact NDJSON pin failed — dispatcher order or wire shape drifted.\n\
             This snapshot guards the multi-rule fedpath that editors /\n\
             watch-exec front-ends consume; any byte-level drift here\n\
             is a wire-format break for downstream consumers. Diff the\n\
             two strings above to localize the change."
        );
    }

    #[test]
    fn test_strict_run_bail_on_unknown_format() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("capsule.toml");
        std::fs::write(
            &manifest_path,
            r#"
[capsule]
name = "test-unknown"
version = "0.1.0"
entry = "main.unknown"
"#,
        )
        .unwrap();

        let entry_path = dir.path().join("main.unknown");
        std::fs::write(&entry_path, "some unknown payload format bytes").unwrap();

        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result_strict = handle_run(vec![], /* strict_mode */ true);
        
        std::env::set_current_dir(old_cwd).unwrap();

        assert!(result_strict.is_err());
        let err_msg = format!("{:#}", result_strict.unwrap_err());
        assert!(err_msg.contains("unknown payload format for entry path"));
        assert!(err_msg.contains("ext: unknown"));
        assert!(err_msg.contains("magic: [73, 6F, 6D, 65]")); // "some" in hex: 73, 6F, 6D, 65
    }
}
