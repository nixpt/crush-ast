//! `crush-installer` — installs and uninstalls the Crush language toolchain.
//!
//! Designed for end-users who want a one-command install of `crushc`,
//! `crush-run`, `crush-compile`, `crush-repl`, and `crush-pkg`.
//!
//! # Examples
//!
//! ```bash
//! # Install to ~/.crush (default)
//! crush-installer install
//!
//! # Install to a custom prefix
//! crush-installer install --prefix /usr/local
//!
//! # Install from a local build directory
//! crush-installer install --bin-dir ./target/release
//!
//! # Skip modifying shell profiles
//! crush-installer install --no-path
//!
//! # Show current installation
//! crush-installer status
//!
//! # Uninstall
//! crush-installer uninstall
//! ```

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use crush_diagnostics::diag_line_from;
use serde::{Deserialize, Serialize};

const BINARIES: &[&str] = &[
    "crushc",
    "crush-run",
    "crush-compile",
    "crush-repl",
    "crush-pkg",
];

const MANIFEST_FILE: &str = "install.json";
const CRUSH_DIR: &str = ".crush";

#[derive(Parser)]
#[command(name = "crush-installer")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Install and manage the Crush language toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Output format for terminal messages (`text` default, `json`
    /// for editor/CI consumers). Matches the `--message-format=json`
    /// dispatch wired into `crush`, `crushc`, `crush-run`,
    /// `crush-compile`, `crush-repl`, `xtask`, `crush-vm`, and
    /// `crush-pkg`. `global = true` lets editors pass the flag
    /// before OR after the subcommand.
    #[arg(long, global = true, value_name = "FORMAT")]
    message_format: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Install the Crush toolchain.
    Install(InstallArgs),
    /// Uninstall the Crush toolchain.
    Uninstall(UninstallArgs),
    /// Show installation status.
    Status(StatusArgs),
}

#[derive(Parser)]
struct InstallArgs {
    /// Installation prefix directory.
    #[arg(short, long, value_name = "DIR")]
    prefix: Option<PathBuf>,

    /// Directory containing prebuilt Crush binaries.
    #[arg(short, long, value_name = "DIR")]
    bin_dir: Option<PathBuf>,

    /// Do not modify shell startup files to add Crush to PATH.
    #[arg(long)]
    no_path: bool,

    /// Force reinstall even if already installed.
    #[arg(short, long)]
    force: bool,

    /// Run quietly.
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Parser)]
struct UninstallArgs {
    /// Installation prefix directory.
    #[arg(short, long, value_name = "DIR")]
    prefix: Option<PathBuf>,

    /// Run quietly.
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Parser)]
struct StatusArgs {
    /// Installation prefix directory.
    #[arg(short, long, value_name = "DIR")]
    prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallManifest {
    version: String,
    install_date: u64,
    prefix: PathBuf,
    bin_dir: PathBuf,
    lib_dir: PathBuf,
    installed_binaries: Vec<String>,
    modified_profiles: Vec<PathBuf>,
}

impl InstallManifest {
    fn path(prefix: &Path) -> PathBuf {
        prefix.join(CRUSH_DIR).join(MANIFEST_FILE)
    }

    fn load(prefix: &Path) -> Result<Option<Self>> {
        let path = Self::path(prefix);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("cannot read manifest '{}'", path.display()))?;
        let manifest: InstallManifest = serde_json::from_str(&content)
            .with_context(|| format!("corrupt install manifest '{}'", path.display()))?;
        Ok(Some(manifest))
    }

    fn save(&self, prefix: &Path) -> Result<()> {
        let dir = prefix.join(CRUSH_DIR);
        fs::create_dir_all(&dir).with_context(|| format!("cannot create '{}'", dir.display()))?;
        let path = Self::path(prefix);
        let content =
            serde_json::to_string_pretty(self).context("failed to serialize install manifest")?;
        fs::write(&path, content)
            .with_context(|| format!("cannot write manifest '{}'", path.display()))?;
        Ok(())
    }
}

fn main() {
    let cli = Cli::parse();
    let json_mode = cli.message_format.as_deref() == Some("json");
    if let Err(e) = run(cli, json_mode) {
        if json_mode {
            emit_diag(CODE_INSTALL, "error", &format!("{e:#}"), None, None);
        } else {
            eprintln!("crush-installer: {e:#}");
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli, json_mode: bool) -> Result<()> {
    match cli.command {
        Commands::Install(args) => install(args),
        Commands::Uninstall(args) => uninstall(args),
        Commands::Status(args) => status(args, &mut std::io::stdout(), json_mode),
    }
}

fn default_prefix() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".crush"))
        .unwrap_or_else(|| PathBuf::from(".crush"))
}

fn discover_bin_dir() -> Option<PathBuf> {
    // If running from cargo target dir, infer it from the current executable path.
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .filter(|p| p.components().any(|c| c.as_os_str() == "target"))
}

fn install(args: InstallArgs) -> Result<()> {
    let prefix = args.prefix.unwrap_or_else(default_prefix);
    let bin_dir = args.bin_dir.or_else(discover_bin_dir).with_context(|| {
        "cannot discover binary directory; please specify --bin-dir".to_string()
    })?;

    if !prefix.exists() {
        fs::create_dir_all(&prefix)
            .with_context(|| format!("cannot create prefix '{}'", prefix.display()))?;
    }

    // Detect existing installation
    if let Some(existing) = InstallManifest::load(&prefix)? {
        if !args.force {
            bail!(
                "Crush is already installed at '{}' ({}). Use --force to reinstall.",
                prefix.display(),
                existing.version
            );
        }
        if !args.quiet {
            eprintln!("Reinstalling Crush at '{}'", prefix.display());
        }
    } else if !args.quiet {
        eprintln!("Installing Crush to '{}'", prefix.display());
    }

    let dest_bin = prefix.join("bin");
    let dest_lib = prefix.join("lib");
    fs::create_dir_all(&dest_bin)
        .with_context(|| format!("cannot create '{}'", dest_bin.display()))?;
    fs::create_dir_all(&dest_lib)
        .with_context(|| format!("cannot create '{}'", dest_lib.display()))?;

    let mut installed = Vec::new();
    for name in BINARIES {
        let src = bin_dir.join(binary_name(name));
        if !src.exists() {
            bail!(
                "binary not found: '{}' (looked in '{}')",
                src.display(),
                bin_dir.display()
            );
        }
        let dst = dest_bin.join(binary_name(name));
        fs::copy(&src, &dst)
            .with_context(|| format!("cannot copy '{}' to '{}'", src.display(), dst.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dst)?.permissions();
            perms.set_mode(perms.mode() | 0o111);
            fs::set_permissions(&dst, perms)?;
        }
        installed.push(binary_name(name).to_string());
        if !args.quiet {
            eprintln!("  installed {}", dst.display());
        }
    }

    let mut modified_profiles = Vec::new();
    if !args.no_path {
        modified_profiles = update_shell_profiles(&dest_bin)?;
        if !args.quiet && !modified_profiles.is_empty() {
            eprintln!("  updated shell profiles:");
            for p in &modified_profiles {
                eprintln!("    {}", p.display());
            }
        }
    }

    let manifest = InstallManifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        install_date: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        prefix: prefix.clone(),
        bin_dir: dest_bin.clone(),
        lib_dir: dest_lib,
        installed_binaries: installed,
        modified_profiles: modified_profiles.clone(),
    };
    manifest.save(&prefix)?;

    if !args.quiet {
        eprintln!("\nCrush {} installed successfully.", manifest.version);
        if !args.no_path {
            eprintln!("Open a new shell or run `source <profile>` to update PATH.");
        } else {
            eprintln!("Add '{}' to your PATH manually.", dest_bin.display());
        }
    }
    Ok(())
}

fn uninstall(args: UninstallArgs) -> Result<()> {
    let prefix = args.prefix.unwrap_or_else(default_prefix);
    let manifest = InstallManifest::load(&prefix)?
        .with_context(|| format!("no Crush installation found at '{}'", prefix.display()))?;

    if !args.quiet {
        eprintln!(
            "Uninstalling Crush {} from '{}'",
            manifest.version,
            prefix.display()
        );
    }

    for binary in &manifest.installed_binaries {
        let path = manifest.bin_dir.join(binary);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("cannot remove '{}'", path.display()))?;
            if !args.quiet {
                eprintln!("  removed {}", path.display());
            }
        }
    }

    for profile in &manifest.modified_profiles {
        if profile.exists() {
            remove_path_from_profile(profile, &manifest.bin_dir)?;
            if !args.quiet {
                eprintln!("  cleaned {}", profile.display());
            }
        }
    }

    // Remove manifest and empty directories
    let manifest_path = InstallManifest::path(&prefix);
    if manifest_path.exists() {
        fs::remove_file(&manifest_path)?;
    }
    let crush_dir = prefix.join(CRUSH_DIR);
    if crush_dir.exists() {
        let _ = fs::remove_dir(&crush_dir);
    }
    let bin_dir = prefix.join("bin");
    if bin_dir.exists() && is_dir_empty(&bin_dir)? {
        let _ = fs::remove_dir(&bin_dir);
    }
    let lib_dir = prefix.join("lib");
    if lib_dir.exists() && is_dir_empty(&lib_dir)? {
        let _ = fs::remove_dir(&lib_dir);
    }
    if prefix != default_prefix() && is_dir_empty(&prefix)? {
        let _ = fs::remove_dir(&prefix);
    }

    if !args.quiet {
        eprintln!(
            "\nCrush uninstalled. You may need to restart your shell for PATH changes to take effect."
        );
    }
    Ok(())
}

fn status(args: StatusArgs, out: &mut dyn Write, json_mode: bool) -> Result<()> {
    let prefix = args.prefix.unwrap_or_else(default_prefix);
    match InstallManifest::load(&prefix)? {
        Some(manifest) => {
            if json_mode {
                // Success path: JSON mode. Emit a headline record
                // then one record per installed binary. MISSING
                // binaries surface as `level: "warning"` so editors
                // can grep the NDJSON stream for install-hygiene
                // issues; present binaries are `level: "note"`.
                // The hint slot carries the install prefix so
                // editor consumers can group status records by
                // destination (mirrors the failure-path hint that
                // install/uninstall already produce).
                use crush_diagnostics::diag_line_from;
                out.write_all(
                    diag_line_from(
                        CODE_INSTALL,
                        "note",
                        "Crush installation found",
                        None,
                        None,
                    )
                    .as_bytes(),
                )?;
                let prefix_str = prefix.display().to_string();
                for binary in &manifest.installed_binaries {
                    let path = manifest.bin_dir.join(binary);
                    let (message, level) = if path.exists() {
                        let version = CommandRunner::version(&path)
                            .unwrap_or_else(|_| "unknown".to_string());
                        (format!("{binary}: {version}"), "note")
                    } else {
                        (format!("{binary}: MISSING"), "warning")
                    };
                    out.write_all(
                        diag_line_from(
                            CODE_INSTALL,
                            level,
                            &message,
                            Some(&prefix_str),
                            None,
                        )
                        .as_bytes(),
                    )?;
                }
                return Ok(());
            }
            // Text mode (unchanged; routed through `out` so the
            // signature stays consistent with the json path).
            // Errors from `out.write_all` are propagated via
            // anyhow's `From<io::Error>`; this is also a silent-bug
            // fix vs the prior `println!` shape (which dropped
            // write errors).
            let date = chrono_datetime(manifest.install_date);
            let mut buf = String::new();
            buf.push_str("Crush installation found\n");
            buf.push_str(&format!("  version:      {}\n", manifest.version));
            buf.push_str(&format!("  installed:    {date}\n"));
            buf.push_str(&format!("  prefix:       {}\n", manifest.prefix.display()));
            buf.push_str(&format!("  bin dir:      {}\n", manifest.bin_dir.display()));
            buf.push_str(&format!("  lib dir:      {}\n", manifest.lib_dir.display()));
            buf.push_str(&format!(
                "  binaries:     {}\n",
                manifest.installed_binaries.join(", ")
            ));
            buf.push_str(&format!(
                "  profiles:     {}\n",
                manifest
                    .modified_profiles
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            for binary in &manifest.installed_binaries {
                let path = manifest.bin_dir.join(binary);
                if path.exists() {
                    let version = CommandRunner::version(&path)
                        .unwrap_or_else(|_| "unknown".to_string());
                    buf.push_str(&format!("  {binary}: {version}\n"));
                } else {
                    buf.push_str(&format!("  {binary}: MISSING\n"));
                }
            }
            out.write_all(buf.as_bytes())?;
        }
        None => {
            if json_mode {
                use crush_diagnostics::diag_line_from;
                let msg = format!("No Crush installation found at '{}'", prefix.display());
                out.write_all(
                    diag_line_from(
                        CODE_INSTALL,
                        "note",
                        &msg,
                        Some("run `crush-installer install` to install"),
                        None,
                    )
                    .as_bytes(),
                )?;
                return Ok(());
            }
            out.write_all(
                format!(
                    "No Crush installation found at '{}'.\nRun `crush-installer install` to install.\n",
                    prefix.display()
                )
                .as_bytes(),
            )?;
        }
    }
    Ok(())
}

fn binary_name(base: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{base}.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        base.to_string()
    }
}

fn is_dir_empty(path: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(path)
        .with_context(|| format!("cannot read directory '{}'", path.display()))?;
    Ok(entries.next().is_none())
}

fn update_shell_profiles(bin_dir: &Path) -> Result<Vec<PathBuf>> {
    update_shell_profiles_with(bin_dir, &shell_profiles())
}

fn update_shell_profiles_with(bin_dir: &Path, profiles: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut modified = Vec::new();
    let bin_path = bin_dir.to_string_lossy();
    let line = format!(r#"export PATH="{}:$PATH""#, bin_path);

    for profile in profiles {
        if !profile.exists() {
            continue;
        }
        let content = fs::read_to_string(profile)
            .with_context(|| format!("cannot read '{}'", profile.display()))?;
        if content.contains(bin_path.as_ref()) {
            continue; // already contains this PATH entry
        }
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(profile)
            .with_context(|| format!("cannot open '{}' for append", profile.display()))?;
        writeln!(file, "\n# Added by crush-installer")?;
        writeln!(file, "{line}")?;
        modified.push(profile.clone());
    }
    Ok(modified)
}

fn remove_path_from_profile(profile: &Path, bin_dir: &Path) -> Result<()> {
    let content = fs::read_to_string(profile)
        .with_context(|| format!("cannot read '{}'", profile.display()))?;
    let bin_path = bin_dir.to_string_lossy();
    let mut filtered: Vec<String> = Vec::new();
    let mut skip_next = false;
    for line in content.lines() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if line.trim() == "# Added by crush-installer" {
            skip_next = true;
            continue;
        }
        if line.contains(&*bin_path) && line.contains("PATH=") {
            continue;
        }
        filtered.push(line.to_string());
    }
    fs::write(profile, filtered.join("\n"))
        .with_context(|| format!("cannot write '{}'", profile.display()))?;
    Ok(())
}

fn shell_profiles() -> Vec<PathBuf> {
    let mut profiles = Vec::new();
    if let Some(home) = dirs::home_dir() {
        profiles.push(home.join(".bashrc"));
        profiles.push(home.join(".zshrc"));
        profiles.push(home.join(".profile"));
        profiles.push(home.join(".bash_profile"));
        profiles.push(home.join(".zprofile"));
        profiles.push(home.join(".config/fish/config.fish"));
    }
    profiles
}

fn chrono_datetime(timestamp: u64) -> String {
    let secs = timestamp as i64;
    let mut year = 1970;
    let mut month = 1;
    let mut day = 1;
    let mut days = secs / 86400;
    let second = (secs % 60) as i32;
    let minute = ((secs % 3600) / 60) as i32;
    let hour = ((secs % 86400) / 3600) as i32;

    while days >= days_in_year(year) {
        days -= days_in_year(year);
        year += 1;
    }
    while days >= days_in_month(year, month) {
        days -= days_in_month(year, month);
        month += 1;
    }
    day += days as i32;

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn days_in_year(year: i32) -> i64 {
    if is_leap_year(year) { 366 } else { 365 }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: i32) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

struct CommandRunner;

impl CommandRunner {
    fn version(path: &Path) -> Result<String> {
        let output = std::process::Command::new(path)
            .arg("--version")
            .output()
            .with_context(|| format!("cannot run '{}' --version", path.display()))?;
        if !output.status.success() {
            bail!("'{} --version' failed", path.display());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

// =========================================================================
// QUARANTINE: per-binary wire-code constant + local emit helper for
// crush-installer.
//
// `DiagRecord` + `diag_line_from` come from the canonical
// `crush_diagnostics` peer crate (extracted from this file in the
// 2026-06-20 peer-extract pass); the local `emit_diag` is now a
// one-line wrapper around `diag_line_from` + `print!` so existing
// call sites don't churn. Wire-shape lockdown (byte-exact field
// order, embedded-quote round-trip, canonical-order assertion)
// lives canonically in
// `crates/crush-diagnostics/tests/wire_format.rs` and is the
// single source of truth across `xtask`, `crush-vm`,
// `crush-installer`, `crush-pkg`.
//
// Keep CODE_INSTALL aligned with the inline-literal convention
// documented in `crush_lang_sdk/src/theme.rs::JsonDiagnostic`.
// =========================================================================

/// Per-binary wire code — emitted on every `crush-installer`
/// failure path (missing prefix, manifest-decode failures,
/// binary-copy errors, profile-update failures). Inline literal
/// paralleling the `E-AUDIT`/`E-LINT` convention documented in
/// `crush_lang_sdk/src/theme.rs::JsonDiagnostic`.
pub const CODE_INSTALL: &str = "E-INSTALL";

/// Thin local wrapper: calls the canonical `diag_line_from` and
/// writes the resulting NDJSON line to stdout. Future edits to the
/// wire shape will surface in
/// `crates/crush-diagnostics/tests/wire_format.rs` simultaneously
/// — the wrapper centralizes the stream choice without
/// re-implementing the seven-field shape.
fn emit_diag(
    code: &str,
    level: &str,
    message: &str,
    file: Option<&str>,
    hint: Option<&str>,
) {
    print!("{}", diag_line_from(code, level, message, hint, file));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_install_creates_manifest_and_copies_binaries() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src_bin");
        let prefix = tmp.path().join("install");
        fs::create_dir_all(&src).unwrap();

        // Create dummy binaries
        for name in BINARIES {
            let path = src.join(binary_name(name));
            fs::write(&path, b"dummy").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut p = fs::metadata(&path).unwrap().permissions();
                p.set_mode(0o644);
                fs::set_permissions(&path, p).unwrap();
            }
        }

        install(InstallArgs {
            prefix: Some(prefix.clone()),
            bin_dir: Some(src),
            no_path: true,
            force: false,
            quiet: true,
        })
        .unwrap();

        let manifest = InstallManifest::load(&prefix).unwrap().unwrap();
        assert_eq!(manifest.installed_binaries.len(), BINARIES.len());
        for name in BINARIES {
            assert!(prefix.join("bin").join(binary_name(name)).exists());
        }
    }

    #[test]
    fn test_uninstall_removes_files() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src_bin");
        let prefix = tmp.path().join("install");
        fs::create_dir_all(&src).unwrap();

        for name in BINARIES {
            fs::write(src.join(binary_name(name)), b"dummy").unwrap();
        }

        install(InstallArgs {
            prefix: Some(prefix.clone()),
            bin_dir: Some(src),
            no_path: true,
            force: false,
            quiet: true,
        })
        .unwrap();

        uninstall(UninstallArgs {
            prefix: Some(prefix.clone()),
            quiet: true,
        })
        .unwrap();

        assert!(InstallManifest::load(&prefix).unwrap().is_none());
        for name in BINARIES {
            assert!(!prefix.join("bin").join(binary_name(name)).exists());
        }
    }

    #[test]
    fn test_shell_profile_path_update_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let profile = tmp.path().join(".bashrc");
        fs::write(&profile, "# existing\n").unwrap();

        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let profiles = vec![profile.clone()];
        let modified1 = update_shell_profiles_with(&bin_dir, &profiles).unwrap();
        assert_eq!(modified1.len(), 1);

        let modified2 = update_shell_profiles_with(&bin_dir, &profiles).unwrap();
        assert_eq!(modified2.len(), 0);

        let content = fs::read_to_string(&profile).unwrap();
        assert!(content.contains(&format!("export PATH=\"{}:$PATH\"", bin_dir.display())));
    }

    #[test]
    fn test_remove_path_from_profile() {
        let tmp = TempDir::new().unwrap();
        let profile = tmp.path().join(".bashrc");
        let bin_dir = tmp.path().join("bin");
        fs::write(
            &profile,
            format!(
                "# before\n# Added by crush-installer\nexport PATH=\"{}:$PATH\"\n# after\n",
                bin_dir.display()
            ),
        )
        .unwrap();

        remove_path_from_profile(&profile, &bin_dir).unwrap();
        let content = fs::read_to_string(&profile).unwrap();
        assert!(!content.contains("crush-installer"));
        assert!(!content.contains(&bin_dir.display().to_string()));
        assert!(content.contains("# before"));
        assert!(content.contains("# after"));
    }

    #[test]
    fn test_chrono_datetime_known_epoch() {
        assert_eq!(chrono_datetime(0), "1970-01-01 00:00:00 UTC");
    }

    // ----------------------------------------------------------------
    // QUARANTINE: import-smoke for the canonical
    // `crush_diagnostics` peer crate. Full wire-format lockdown
    // (byte-exact field order, embedded-quote round-trip,
    // canonical-order assertion) lives in
    // `crates/crush-diagnostics/tests/wire_format.rs`. The test
    // here only confirms the import path resolves so a future
    // rename of the canonical crate surfaces synchronously.
    // ----------------------------------------------------------------

    #[test]
    fn install_smoke_crush_diagnostics_resolves() {
        use crush_diagnostics::{DiagRecord, diag_line_from};
        // Construct a DiagRecord so dead-code analysis doesn't
        // strip the import between rustc runs.
        let _rec = DiagRecord {
            code: CODE_INSTALL,
            level: "error",
            file: None,
            line: None,
            col: None,
            message: "smoke",
            hint: None,
        };
        let line = diag_line_from(CODE_INSTALL, "error", "smoke", None, None);
        let expected = concat!(
            r#"{"code":"E-INSTALL","level":"error","file":null,"line":null,"col":null,"message":"smoke","hint":null}"#,
            "\n"
        );
        assert_eq!(line, expected);
    }

    // ----------------------------------------------------------------
    // Status JSON-mode wire-through lockdown
    // ----------------------------------------------------------------
    //
    // The status subcommand used to short-circuit on the success
    // path (text mode only); after the 2026-06-20 wire-through,
    // both branches emit ndjson records when `--message-format=json`
    // is set. These tests pin the byte-exact shape of the records
    // so a future refactor that breaks the seven-field layout or
    // drops a per-binary row fails synchronously here AND at the
    // canonical lockdown in
    // `crates/crush-diagnostics/tests/wire_format.rs`.
    //
    // The text-mode branch is pinned in
    // `status_text_mode_emits_human_readable_when_json_mode_disabled`.
    // ----------------------------------------------------------------

    #[test]
    fn status_emits_ndjson_record_for_absent_installation_in_json_mode() {
        // No manifest on disk — InstallManifest::load returns
        // Ok(None) — so the status subcommand flows through the
        // headlining-only branch.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("install");
        let mut out = Vec::<u8>::new();
        status(
            StatusArgs {
                prefix: Some(prefix.clone()),
            },
            &mut out,
            true,
        )
        .unwrap();
        let s = std::str::from_utf8(&out).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "absent status must emit exactly one ndjson record (got {lines:?})"
        );
        let v: serde_json::Value = serde_json::from_str(lines[0])
            .expect("headline ndjson record must round-trip serde");
        assert_eq!(v["code"], "E-INSTALL");
        assert_eq!(v["level"], "note");
        let msg = v["message"].as_str().expect("message is a json string");
        assert!(
            msg.contains("No Crush installation found"),
            "headline message must include 'No Crush installation found' (got: {msg:?})"
        );
        assert!(
            msg.contains(&prefix.display().to_string()),
            "headline message must include the prefix path (got: {msg:?})"
        );
        assert_eq!(
            v["hint"].as_str(),
            Some("run `crush-installer install` to install"),
            "headline hint must suggest `crush-installer install`"
        );
    }

    #[test]
    fn status_emits_ndjson_records_for_present_installation_in_json_mode() {
        // Install a manifest with 3 binaries; only 2 have actual
        // on-disk scripts. Exercises both the note-level present
        // branch AND the warning-level missing branch in one test
        // so the per-binary probe wiring is fully covered.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("install");
        let bin_dir = tmp.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let manifest_binaries = vec![
            "crushc".to_string(),
            "crush-pkg".to_string(),
            "crush-run".to_string(),
        ];
        for binary in &manifest_binaries[..2] {
            let p = bin_dir.join(binary);
            #[cfg(unix)]
            {
                fs::write(&p, "#!/bin/sh\necho \"v0.0.0-test\"\n").unwrap();
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&p).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&p, perms).unwrap();
            }
            #[cfg(not(unix))]
            {
                fs::write(&p, b"@echo v0.0.0-test\n").unwrap();
            }
        }
        let manifest = InstallManifest {
            version: env!("CARGO_PKG_VERSION").to_string(),
            install_date: 0,
            prefix: prefix.clone(),
            bin_dir: bin_dir.clone(),
            lib_dir: prefix.join("lib"),
            installed_binaries: manifest_binaries,
            modified_profiles: vec![],
        };
        manifest.save(&prefix).unwrap();

        let mut out = Vec::<u8>::new();
        status(
            StatusArgs {
                prefix: Some(prefix.clone()),
            },
            &mut out,
            true,
        )
        .unwrap();
        let s = std::str::from_utf8(&out).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        // 1 headline + 3 per-binary records.
        assert_eq!(
            lines.len(),
            4,
            "present status must emit 1 headline + N per-binary records (got {lines:?})"
        );
        let headline: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(headline["code"], "E-INSTALL");
        assert_eq!(headline["level"], "note");
        assert_eq!(headline["message"], "Crush installation found");
        assert!(
            headline["hint"].is_null(),
            "headline hint slot must be null (was: {:?})",
            headline["hint"]
        );

        let mut notes = 0;
        let mut warnings = 0;
        for line in &lines[1..] {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["code"], "E-INSTALL");
            assert_eq!(
                v["hint"].as_str(),
                Some(prefix.display().to_string().as_str()),
                "per-binary hint must carry the install prefix"
            );
            let msg = v["message"].as_str().expect("message is a json string");
            match v["level"].as_str() {
                Some("note") => {
                    assert!(
                        msg.contains(':'),
                        "note-level message must be 'binary: version' shape (got: {msg:?})"
                    );
                    notes += 1;
                }
                Some("warning") => {
                    assert!(
                        msg.contains("MISSING"),
                        "warning-level message must include MISSING (got: {msg:?})"
                    );
                    warnings += 1;
                }
                other => panic!("unexpected level: {other:?}"),
            }
        }
        assert_eq!(notes, 2, "two present binaries expected 2 note records");
        assert_eq!(warnings, 1, "one missing binary expected 1 warning record");
    }

    #[test]
    fn status_text_mode_emits_human_readable_when_json_mode_disabled() {
        // Smoke: the `&mut dyn Write` reshape does not perturb
        // text-mode byte-for-byte. Empty `installed_binaries` so
        // the test is short and skips the per-binary tail.
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path().join("install");
        let manifest = InstallManifest {
            version: "0.1.0-test".to_string(),
            install_date: 0,
            prefix: prefix.clone(),
            bin_dir: prefix.join("bin"),
            lib_dir: prefix.join("lib"),
            installed_binaries: vec![],
            modified_profiles: vec![],
        };
        manifest.save(&prefix).unwrap();
        let mut out = Vec::<u8>::new();
        status(
            StatusArgs {
                prefix: Some(prefix.clone()),
            },
            &mut out,
            false,
        )
        .unwrap();
        let s = std::str::from_utf8(&out).unwrap();
        assert!(s.starts_with("Crush installation found\n"));
        assert!(s.contains("  version:      0.1.0-test\n"));
        assert!(s.contains("  installed:    1970-01-01 00:00:00 UTC\n"));
        assert!(s.contains("  prefix:       "));
        // Text-mode must contain ZERO ndjson records; the
        // json-mode path is gated behind the `json_mode` flag.
        for line in s.lines() {
            assert!(
                !line.starts_with('{'),
                "text-mode output must not contain ndjson records (got: {line:?})"
            );
        }
    }
}
