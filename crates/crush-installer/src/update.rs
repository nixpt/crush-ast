//! `crush-installer update` — update an installed toolchain from the GitHub
//! release binaries (CRUSH-344). `crush update` forwards here.
//!
//! The steps, in order, and what each one guarantees:
//!
//! 1. **Target.** Only `x86_64-unknown-linux-gnu` has a prebuilt tarball
//!    (`.github/workflows/artifacts.yml`); anything else is refused with a
//!    pointer at `cargo install crush-lang-sdk` ([`host_target`]).
//! 2. **Install dir.** `--dir`, else the directory the running toolchain lives
//!    in if the installer manages it (or cargo does — so the refusal below can
//!    fire), else `~/.crush/bin` ([`resolve_dir`]).
//! 3. **Cargo-managed?** Binaries in `$CARGO_HOME/bin` or listed in a
//!    `.crates2.json` are cargo's: refused unless `--force` ([`cargo_managed`]).
//! 4. **Version.** The installed `crush --version` against the newest
//!    published release (or `--version`). Never a downgrade unless `--version`
//!    names the older one.
//! 5. **Download + verify.** `SHA256SUMS` first; the tarball is streamed into a
//!    staging dir *inside the install dir* (same filesystem) while being hashed,
//!    and compared before anything is extracted ([`download_verified`]).
//! 6. **Replace.** Every binary is staged first; then each old one is
//!    hard-linked into a backup and the new one renamed over it. A failed
//!    rename puts every already-replaced binary back from its backup, so the
//!    install is either all-old or all-new ([`replace_all`]). `rename(2)` over
//!    a running executable is fine on Linux — the running process keeps its
//!    inode — so `crush update` can replace the `crush` that started it.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use sha2::{Digest, Sha256};

use crate::{BINARIES, InstallManifest, OPTIONAL_BINARIES, binary_name, default_prefix};

/// `--check` exit status when a newer release (or the pinned `--version`) is
/// available. 0 = up to date, 1 = error.
pub const EXIT_UPDATE_AVAILABLE: i32 = 10;

/// The one target the release workflow builds.
pub const SUPPORTED_TARGET: &str = "x86_64-unknown-linux-gnu";

const RELEASES_URL: &str = "https://api.github.com/repos/nixpt/crush-ast/releases?per_page=100";
const DOWNLOAD_BASE: &str = "https://github.com/nixpt/crush-ast/releases/download";
const SUMS_FILE: &str = "SHA256SUMS";
/// Upper bound on any single download (the v0.9.0 tarball is well under it).
const MAX_DOWNLOAD: u64 = 512 << 20;

/// Shipped in the tarball and replaced when present there, never required:
/// `crush-installer` itself is not in the v0.9.0 tarball, but if a later one
/// carries it, it is updated like the rest.
const UPDATE_OPTIONAL: &[&str] = &["crush-installer"];

/// What `crush update` tells the user to run instead when the updater can't help.
const CARGO_HINT: &str = "cargo install crush-lang-sdk";

#[derive(Parser, Debug)]
pub struct UpdateArgs {
    /// Only report the installed and the available version; write nothing.
    /// Exits 0 when up to date, 10 when an update is available.
    #[arg(long)]
    pub check: bool,

    /// Install this version (X.Y.Z) instead of the newest release. The only
    /// way to downgrade.
    #[arg(long, value_name = "X.Y.Z")]
    pub version: Option<String>,

    /// Directory holding the binaries to update (default: the running
    /// toolchain's directory if crush-installer manages it, else ~/.crush/bin).
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Replace even a cargo-managed install, and reinstall the same version.
    #[arg(long)]
    pub force: bool,

    /// Accepted for scripts; the updater never prompts.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Print only errors (and the `--check` report).
    #[arg(short, long)]
    pub quiet: bool,

    /// Directory of the `crush` that forwarded `crush update` (set by the
    /// umbrella CLI, not by users).
    #[arg(long, hide = true, value_name = "DIR")]
    pub caller_dir: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PreIdent {
    // Variant order is the SemVer order: numeric identifiers sort before
    // alphanumeric ones.
    Num(u64),
    Alpha(String),
}

/// A SemVer 2.0 version (build metadata is accepted and ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PreIdent>,
    pre_text: String,
}

impl Version {
    /// Parse `X.Y.Z`, `vX.Y.Z`, `X.Y.Z-pre.1`, `X.Y.Z+build`.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let core = s.strip_prefix('v').unwrap_or(s);
        let core = core.split_once('+').map_or(core, |(c, _)| c);
        let (core, pre_text) = match core.split_once('-') {
            Some((_, "")) => bail!("empty pre-release in '{s}'"),
            Some(split) => split,
            None => (core, ""),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            bail!("not a version (want X.Y.Z): '{s}'");
        }
        let num = |p: &str| {
            p.parse::<u64>()
                .with_context(|| format!("not a version (want X.Y.Z): '{s}'"))
        };
        let pre = if pre_text.is_empty() {
            Vec::new()
        } else {
            pre_text
                .split('.')
                .map(|id| {
                    if id.is_empty() {
                        bail!("empty pre-release identifier in '{s}'");
                    }
                    Ok(match id.parse::<u64>() {
                        Ok(n) if id.bytes().all(|b| b.is_ascii_digit()) => PreIdent::Num(n),
                        _ => PreIdent::Alpha(id.to_string()),
                    })
                })
                .collect::<Result<_>>()?
        };
        Ok(Version {
            major: num(parts[0])?,
            minor: num(parts[1])?,
            patch: num(parts[2])?,
            pre,
            pre_text: pre_text.to_string(),
        })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::Equal;
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                // A release outranks every pre-release of the same X.Y.Z.
                (true, true) => Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                // Identifier by identifier; a shorter prefix sorts first.
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre_text.is_empty() {
            write!(f, "-{}", self.pre_text)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Target, asset names, SHA256SUMS
// ---------------------------------------------------------------------------

/// The release target for this host, or a refusal naming it.
pub fn host_target() -> Result<&'static str> {
    target_for(
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(target_env = "gnu"),
    )
}

fn target_for(arch: &str, os: &str, gnu: bool) -> Result<&'static str> {
    if arch == "x86_64" && os == "linux" && gnu {
        return Ok(SUPPORTED_TARGET);
    }
    let env = if os == "linux" && !gnu { "-musl" } else { "" };
    bail!(
        "no prebuilt Crush release for {arch}-{os}{env} (only {SUPPORTED_TARGET} is published); \
         build from source instead: {CARGO_HINT}"
    )
}

pub fn tarball_name(v: &Version, target: &str) -> String {
    format!("crush-{v}-{target}.tar.gz")
}

/// Parse `sha256sum` output: `<64 hex>  <name>` (or ` *<name>`, binary mode).
pub fn parse_sha256sums(text: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let (hash, rest) = line
            .split_once(' ')
            .with_context(|| format!("SHA256SUMS line {}: malformed: '{line}'", n + 1))?;
        let name = rest.trim_start_matches(' ');
        let name = name.strip_prefix('*').unwrap_or(name);
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) || name.is_empty() {
            bail!("SHA256SUMS line {}: malformed: '{line}'", n + 1);
        }
        out.push((hash.to_ascii_lowercase(), name.to_string()));
    }
    Ok(out)
}

/// The one expected hash for `name`; refuses a missing or contradictory entry.
pub fn expected_sha256(sums: &[(String, String)], name: &str) -> Result<String> {
    let mut found: Option<&str> = None;
    for (hash, n) in sums {
        if n == name {
            match found {
                Some(prev) if prev != hash => {
                    bail!("SHA256SUMS lists '{name}' twice with different hashes; refusing")
                }
                _ => found = Some(hash),
            }
        }
    }
    found
        .map(str::to_string)
        .with_context(|| format!("SHA256SUMS has no entry for '{name}'; refusing to install it"))
}

// ---------------------------------------------------------------------------
// Fetching (https via ureq, file:// for tests and local mirrors)
// ---------------------------------------------------------------------------

/// Where releases come from. `CRUSH_UPDATE_RELEASES_URL` /
/// `CRUSH_UPDATE_DOWNLOAD_BASE` override the GitHub URLs (mirrors, tests;
/// `file://` works).
pub struct Source {
    pub releases_url: String,
    pub download_base: String,
    pub token: Option<String>,
}

impl Source {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Source {
            releases_url: var("CRUSH_UPDATE_RELEASES_URL").unwrap_or_else(|| RELEASES_URL.into()),
            download_base: var("CRUSH_UPDATE_DOWNLOAD_BASE")
                .unwrap_or_else(|| DOWNLOAD_BASE.into()),
            token: var("GITHUB_TOKEN"),
        }
    }

    fn asset_url(&self, v: &Version, name: &str) -> String {
        format!("{}/v{v}/{name}", self.download_base.trim_end_matches('/'))
    }

    /// Stream `url` into `sink`, at most `MAX_DOWNLOAD` bytes.
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u64> {
        let reader: Box<dyn Read> = if let Some(path) = url.strip_prefix("file://") {
            Box::new(File::open(path).with_context(|| format!("not found: {url}"))?)
        } else {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(30))
                .timeout_read(Duration::from_secs(120))
                .build();
            let mut req = agent.get(url).set(
                "User-Agent",
                concat!("crush-installer/", env!("CARGO_PKG_VERSION")),
            );
            // The token only buys API rate limit; it is never sent to the
            // download host (and ureq drops it on cross-host redirects anyway).
            if url.starts_with("https://api.github.com/") {
                req = req.set("Accept", "application/vnd.github+json");
                if let Some(t) = &self.token {
                    req = req.set("Authorization", &format!("Bearer {t}"));
                }
            }
            match req.call() {
                Ok(resp) => Box::new(resp.into_reader()),
                Err(ureq::Error::Status(404, _)) => bail!("not found (HTTP 404): {url}"),
                Err(ureq::Error::Status(code @ (403 | 429), _)) => bail!(
                    "HTTP {code} from {url} (rate limited? set GITHUB_TOKEN to raise the limit)"
                ),
                Err(ureq::Error::Status(code, _)) => bail!("HTTP {code} from {url}"),
                Err(e) => return Err(anyhow::Error::new(e).context(format!("cannot fetch {url}"))),
            }
        };
        let n = io::copy(&mut reader.take(MAX_DOWNLOAD + 1), sink)
            .with_context(|| format!("download of {url} failed"))?;
        if n > MAX_DOWNLOAD {
            bail!("{url} is larger than {MAX_DOWNLOAD} bytes; refusing");
        }
        Ok(n)
    }

    fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.fetch(url, &mut buf)?;
        Ok(buf)
    }

    /// The newest non-draft, non-SemVer-pre-release release that carries a
    /// tarball for `target` and a `SHA256SUMS`. GitHub's own `prerelease`
    /// flag is ignored on purpose: every pre-1.0 release is flagged so
    /// (CRUSH-342), and `/releases/latest` skips flagged ones.
    pub fn latest(&self, target: &str) -> Result<Version> {
        let body = self.fetch_bytes(&self.releases_url)?;
        let releases: Vec<serde_json::Value> = serde_json::from_slice(&body)
            .with_context(|| format!("unexpected release list from {}", self.releases_url))?;
        let mut best: Option<Version> = None;
        for r in &releases {
            if r["draft"].as_bool() == Some(true) {
                continue;
            }
            let Some(v) = r["tag_name"].as_str().and_then(|t| Version::parse(t).ok()) else {
                continue;
            };
            if v.is_prerelease() {
                continue;
            }
            let assets: Vec<&str> = r["assets"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect())
                .unwrap_or_default();
            if !assets.contains(&tarball_name(&v, target).as_str()) || !assets.contains(&SUMS_FILE)
            {
                continue;
            }
            if best.as_ref().is_none_or(|b| v > *b) {
                best = Some(v);
            }
        }
        best.with_context(|| {
            format!(
                "no release with a {target} build found at {}",
                self.releases_url
            )
        })
    }
}

struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Download `name` of release `v` into `dest`, verifying it against that
/// release's `SHA256SUMS`. On a missing entry or a mismatch `dest` is removed
/// and an error returned — nothing downstream ever sees unverified bytes.
pub fn download_verified(src: &Source, v: &Version, name: &str, dest: &Path) -> Result<()> {
    let sums = src
        .fetch_bytes(&src.asset_url(v, SUMS_FILE))
        .with_context(|| {
            format!("cannot get SHA256SUMS for v{v} (is v{v} a published release?)")
        })?;
    let sums = parse_sha256sums(&String::from_utf8_lossy(&sums))?;
    let want = expected_sha256(&sums, name)?;

    let file = File::create(dest).with_context(|| format!("cannot create '{}'", dest.display()))?;
    let mut w = HashingWriter {
        inner: io::BufWriter::new(file),
        hasher: Sha256::new(),
    };
    let fetched = src.fetch(&src.asset_url(v, name), &mut w);
    let flushed = w.flush();
    let got = hex(&w.hasher.finalize());
    if let Err(e) = fetched.and_then(|_| flushed.map_err(Into::into)) {
        let _ = fs::remove_file(dest);
        return Err(e);
    }
    if got != want {
        let _ = fs::remove_file(dest);
        bail!(
            "checksum mismatch for {name}: SHA256SUMS says {want}, downloaded file is {got}; refusing to install it"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Install dir: who manages it, what is in it
// ---------------------------------------------------------------------------

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))
}

fn toolchain_bins() -> impl Iterator<Item = &'static str> {
    BINARIES.iter().chain(OPTIONAL_BINARIES).copied()
}

/// `Some(reason)` when `dir`'s Crush binaries belong to `cargo install`:
/// `dir` is `<cargo_home>/bin` holding a `crush`, or a `.crates2.json` in
/// `<cargo_home>` / `dir/..` (cargo's `--root` layout) lists a toolchain
/// binary installed into `dir`.
pub fn cargo_managed(dir: &Path, cargo_home: Option<&Path>) -> Option<String> {
    if let Some(home) = cargo_home
        && same_path(dir, &home.join("bin"))
        && dir.join(binary_name("crush")).exists()
    {
        return Some(format!("'{}' is cargo's bin directory", dir.display()));
    }
    let roots = cargo_home.map(Path::to_path_buf).into_iter().chain(
        dir.parent()
            .filter(|_| dir.ends_with("bin"))
            .map(Path::to_path_buf),
    );
    for root in roots {
        if !same_path(&root.join("bin"), dir) {
            continue;
        }
        let ledger = root.join(".crates2.json");
        let Ok(text) = fs::read_to_string(&ledger) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(installs) = json["installs"].as_object() else {
            continue;
        };
        for (pkg, entry) in installs {
            let bins = entry["bins"].as_array().into_iter().flatten();
            if bins
                .filter_map(|b| b.as_str())
                .any(|b| toolchain_bins().any(|t| binary_name(t) == b))
            {
                let pkg = pkg.split(' ').next().unwrap_or(pkg);
                return Some(format!("{} lists {pkg}", ledger.display()));
            }
        }
    }
    None
}

/// `dir` is `<prefix>/bin` of a `crush-installer install` (its manifest says so).
pub fn installer_managed(dir: &Path) -> bool {
    dir.parent()
        .and_then(|prefix| InstallManifest::load(prefix).ok().flatten())
        .is_some_and(|m| same_path(&m.bin_dir, dir))
}

/// `--dir`, else the running toolchain's dir if the installer (or cargo — so
/// the cargo refusal fires instead of silently updating some other copy)
/// manages it, else `~/.crush/bin`.
pub fn resolve_dir(
    explicit: Option<PathBuf>,
    running: Option<PathBuf>,
    cargo_home: Option<&Path>,
) -> PathBuf {
    if let Some(d) = explicit {
        return d;
    }
    if let Some(r) = running
        && (installer_managed(&r) || cargo_managed(&r, cargo_home).is_some())
    {
        return r;
    }
    default_prefix().join("bin")
}

/// The version `dir/crush --version` reports, `None` when there is no
/// runnable `crush` there.
pub fn installed_version(dir: &Path) -> Option<Version> {
    let out = Command::new(dir.join(binary_name("crush")))
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .last()
        .and_then(|t| Version::parse(t).ok())
}

// ---------------------------------------------------------------------------
// Extract + atomic replace
// ---------------------------------------------------------------------------

/// Extract the verified tarball into `into`, check it has every required
/// binary as a regular file, mark them executable, and return
/// `(name, staged path)` for each binary to install.
pub fn stage_binaries(
    tarball: &Path,
    v: &Version,
    target: &str,
    into: &Path,
) -> Result<Vec<(String, PathBuf)>> {
    let gz = flate2::read::GzDecoder::new(
        File::open(tarball).with_context(|| format!("cannot open '{}'", tarball.display()))?,
    );
    // tar's unpack refuses entries that would land outside `into`.
    tar::Archive::new(gz)
        .unpack(into)
        .with_context(|| format!("cannot extract '{}'", tarball.display()))?;
    let top = into.join(format!("crush-{v}-{target}"));
    if !top.is_dir() {
        bail!("tarball has no top-level 'crush-{v}-{target}/' directory");
    }
    let mut staged = Vec::new();
    let wanted = BINARIES.iter().map(|b| (*b, true)).chain(
        OPTIONAL_BINARIES
            .iter()
            .chain(UPDATE_OPTIONAL)
            .map(|b| (*b, false)),
    );
    for (base, required) in wanted {
        let name = binary_name(base);
        let path = top.join(&name);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => {}
            Ok(_) => bail!("'{name}' in the tarball is not a regular file; refusing"),
            Err(_) if !required => continue,
            Err(_) => bail!("the release tarball has no '{name}'; refusing a partial update"),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
        staged.push((name, path));
    }
    Ok(staged)
}

/// Replace `dir/<name>` with each staged file, all or nothing.
///
/// Phase 1 hard-links every existing target into `backup` (copy where links
/// aren't possible) — `dir` is untouched. Phase 2 `rename`s each staged file
/// over its target; each rename is atomic, so no binary is ever half-written.
/// If one fails, the targets already replaced are renamed back from their
/// backups (or removed, if they did not exist before) — the old set stays.
/// `before_rename` is a test hook; it runs before each phase-2 rename.
pub fn replace_all(
    dir: &Path,
    staged: &[(String, PathBuf)],
    backup: &Path,
    before_rename: &mut dyn FnMut(&str) -> io::Result<()>,
) -> Result<()> {
    fs::create_dir_all(backup)?;
    let mut existed = Vec::with_capacity(staged.len());
    for (name, _) in staged {
        let target = dir.join(name);
        if fs::symlink_metadata(&target).is_ok() {
            let saved = backup.join(name);
            if fs::hard_link(&target, &saved).is_err() {
                fs::copy(&target, &saved).with_context(|| {
                    format!("cannot back up '{}'; nothing was changed", target.display())
                })?;
            }
            existed.push(true);
        } else {
            existed.push(false);
        }
    }
    for (i, (name, src)) in staged.iter().enumerate() {
        let target = dir.join(name);
        let step = before_rename(name).and_then(|()| fs::rename(src, &target));
        if let Err(e) = step {
            let mut problems = Vec::new();
            for j in (0..i).rev() {
                let t = dir.join(&staged[j].0);
                let undo = if existed[j] {
                    fs::rename(backup.join(&staged[j].0), &t)
                } else {
                    fs::remove_file(&t)
                };
                if let Err(u) = undo {
                    problems.push(format!("{}: {u}", t.display()));
                }
            }
            if problems.is_empty() {
                bail!(
                    "cannot replace '{}': {e}; rolled back, the previous binaries are unchanged",
                    target.display()
                );
            }
            bail!(
                "cannot replace '{}': {e}; ROLLBACK INCOMPLETE ({}) — backups are in '{}'",
                target.display(),
                problems.join("; "),
                backup.display()
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The command
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    UpdateAvailable,
    Updated,
}

impl Outcome {
    pub fn exit_code(&self) -> i32 {
        match self {
            Outcome::UpdateAvailable => EXIT_UPDATE_AVAILABLE,
            Outcome::UpToDate | Outcome::Updated => 0,
        }
    }
}

pub struct Options {
    pub check: bool,
    pub pin: Option<Version>,
    pub dir: PathBuf,
    pub force: bool,
    pub quiet: bool,
}

fn show(v: &Option<Version>) -> String {
    v.as_ref()
        .map_or_else(|| "none".to_string(), Version::to_string)
}

/// Entry point from `main`: resolves everything from the environment.
pub fn update(args: UpdateArgs) -> Result<i32> {
    let target = host_target()?;
    let home = cargo_home();
    let running = args.caller_dir.clone().or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
    });
    let opts = Options {
        check: args.check,
        pin: args.version.as_deref().map(Version::parse).transpose()?,
        dir: resolve_dir(args.dir, running, home.as_deref()),
        force: args.force,
        quiet: args.quiet,
    };
    let mut out: Box<dyn Write> = if opts.check {
        Box::new(io::stdout())
    } else {
        Box::new(io::stderr())
    };
    let outcome = run_update(
        &opts,
        &Source::from_env(),
        target,
        home.as_deref(),
        &mut *out,
        &mut |_| Ok(()),
    )?;
    Ok(outcome.exit_code())
}

/// The whole update, with every input injected (tests use `file://` sources).
pub fn run_update(
    opts: &Options,
    src: &Source,
    target: &str,
    cargo_home: Option<&Path>,
    out: &mut dyn Write,
    before_rename: &mut dyn FnMut(&str) -> io::Result<()>,
) -> Result<Outcome> {
    let dir = &opts.dir;
    let cargo = cargo_managed(dir, cargo_home);
    if let Some(why) = &cargo
        && !opts.check
        && !opts.force
    {
        bail!(
            "{}: installed with cargo; run: {CARGO_HINT} (or pass --force to overwrite cargo's binaries; {why})",
            dir.display()
        );
    }

    let current = installed_version(dir);
    let (wanted, explicit) = match &opts.pin {
        Some(v) => (v.clone(), true),
        None => (src.latest(target)?, false),
    };
    let newer = current.as_ref().is_none_or(|c| wanted > *c);
    let needed = match &current {
        _ if opts.force => true,
        None => true,
        Some(c) if explicit => wanted != *c,
        Some(_) => newer,
    };

    if opts.check {
        writeln!(out, "installed: {} ({})", show(&current), dir.display())?;
        writeln!(
            out,
            "{}: {wanted}",
            if explicit { "requested" } else { "latest" }
        )?;
        if cargo.is_some() {
            writeln!(out, "note: installed with cargo; run: {CARGO_HINT}")?;
        }
        let available = needed && (explicit || newer);
        writeln!(
            out,
            "{}",
            if available {
                format!("update available: {} -> {wanted}", show(&current))
            } else {
                "up to date".to_string()
            }
        )?;
        return Ok(if available {
            Outcome::UpdateAvailable
        } else {
            Outcome::UpToDate
        });
    }

    if !needed {
        if !opts.quiet {
            match &current {
                Some(c) if *c > wanted => writeln!(
                    out,
                    "crush {c} in '{}' is newer than the latest release {wanted}; not downgrading (use --version {wanted} to)",
                    dir.display()
                )?,
                _ => writeln!(
                    out,
                    "crush {} in '{}' is up to date",
                    show(&current),
                    dir.display()
                )?,
            }
        }
        return Ok(Outcome::UpToDate);
    }

    if !dir.is_dir() {
        bail!(
            "'{}' does not exist; install first (`crush-installer install`) or pass --dir",
            dir.display()
        );
    }
    if !opts.quiet {
        let verb = match &current {
            Some(c) if wanted < *c => "downgrading",
            Some(c) if wanted == *c => "reinstalling",
            _ => "updating",
        };
        writeln!(
            out,
            "{verb} crush {} -> {wanted} in '{}'",
            show(&current),
            dir.display()
        )?;
    }

    // Staging lives inside `dir` so every rename below stays on one filesystem;
    // dropping it (success or any error) removes the download and the backups.
    let stage = tempfile::Builder::new()
        .prefix(".crush-update-")
        .tempdir_in(dir)
        .with_context(|| format!("cannot create a staging dir in '{}'", dir.display()))?;
    let name = tarball_name(&wanted, target);
    let tarball = stage.path().join(&name);
    download_verified(src, &wanted, &name, &tarball)?;
    if !opts.quiet {
        writeln!(
            out,
            "  downloaded {name} (sha256 verified against SHA256SUMS)"
        )?;
    }
    let staged = stage_binaries(&tarball, &wanted, target, &stage.path().join("x"))?;
    replace_all(dir, &staged, &stage.path().join("backup"), before_rename)?;
    if !opts.quiet {
        for (n, _) in &staged {
            writeln!(out, "  replaced {}", dir.join(n).display())?;
        }
    }

    // Keep the installer's manifest honest; the binaries are already in place,
    // so a failure here is a warning, not a rollback.
    if installer_managed(dir)
        && let Some(prefix) = dir.parent()
        && let Ok(Some(mut m)) = InstallManifest::load(prefix)
    {
        m.version = wanted.to_string();
        for (n, _) in &staged {
            if !m.installed_binaries.contains(n) {
                m.installed_binaries.push(n.clone());
            }
        }
        if let Err(e) = m.save(prefix) {
            writeln!(
                out,
                "warning: binaries updated but the manifest was not: {e:#}"
            )?;
        }
    }
    if !opts.quiet {
        writeln!(out, "crush {wanted} installed in '{}'", dir.display())?;
    }
    Ok(Outcome::Updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn version_ordering_follows_semver_including_prereleases() {
        assert!(v("0.9.0") > v("0.8.9"));
        assert!(v("0.10.0") > v("0.9.0"), "numeric, not lexical");
        assert!(v("1.0.0") > v("0.99.99"));
        assert_eq!(v("v0.9.0"), v("0.9.0"));
        assert_eq!(
            v("0.9.0+build.5").cmp(&v("0.9.0")),
            std::cmp::Ordering::Equal
        );
        // The SemVer 2.0 §11 example chain.
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for w in chain.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
        }
        assert!(v("1.0.0-rc.1").is_prerelease());
        assert_eq!(v("1.0.0-rc.1").to_string(), "1.0.0-rc.1");
        for bad in ["", "1.0", "1.0.0.0", "a.b.c", "1.0.0-", "1.0.0-a..b"] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn target_refusal_names_the_target_and_points_at_cargo() {
        assert_eq!(
            target_for("x86_64", "linux", true).unwrap(),
            SUPPORTED_TARGET
        );
        for (arch, os, gnu) in [
            ("aarch64", "linux", true),
            ("x86_64", "macos", false),
            ("x86_64", "linux", false),
        ] {
            let e = target_for(arch, os, gnu).unwrap_err().to_string();
            assert!(e.contains(arch) && e.contains(os), "{e}");
            assert!(e.contains("cargo install crush-lang-sdk"), "{e}");
        }
    }

    const REAL_SUMS: &str = "7a57d2abb7effa5d194b1593b5c172e2d98df4848ab4184a6c72ddeb53e2a022  crush-0.9.0-x86_64-unknown-linux-gnu.tar.gz\n77d73be726e75f88c85cc4f50e9bb780586e506c3ec263088ba1621a4e502242  crush-web-0.9.0-pkg.tar.gz\n";

    #[test]
    fn sha256sums_parsing() {
        let sums = parse_sha256sums(REAL_SUMS).unwrap();
        assert_eq!(sums.len(), 2);
        assert_eq!(
            expected_sha256(&sums, "crush-0.9.0-x86_64-unknown-linux-gnu.tar.gz").unwrap(),
            "7a57d2abb7effa5d194b1593b5c172e2d98df4848ab4184a6c72ddeb53e2a022"
        );
        // binary-mode marker and upper-case hex
        let s = parse_sha256sums(&format!("{} *a.tar.gz\n\n", "AB".repeat(32))).unwrap();
        assert_eq!(s, vec![("ab".repeat(32), "a.tar.gz".to_string())]);
        // missing entry
        let e = expected_sha256(&sums, "crush-1.0.0-x.tar.gz")
            .unwrap_err()
            .to_string();
        assert!(e.contains("no entry"), "{e}");
        // contradictory entries
        let dup =
            parse_sha256sums(&format!("{}  a\n{}  a\n", "0".repeat(64), "1".repeat(64))).unwrap();
        assert!(expected_sha256(&dup, "a").is_err());
        // malformed lines
        assert!(parse_sha256sums("deadbeef  a\n").is_err());
        assert!(parse_sha256sums("nospace\n").is_err());
    }

    // --- fixtures: a fake release served over file:// ----------------------

    /// A shell script standing in for a binary: `--version` prints `<name> <ver>`.
    fn script(name: &str, ver: &str) -> Vec<u8> {
        format!("#!/bin/sh\necho \"{name} {ver}\"\n").into_bytes()
    }

    fn make_tarball(ver: &str, bins: &[&str]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        {
            let mut tb = tar::Builder::new(&mut gz);
            let top = format!("crush-{ver}-{SUPPORTED_TARGET}");
            for b in bins {
                let data = script(b, ver);
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o644); // the updater must set +x itself
                h.set_cksum();
                tb.append_data(&mut h, format!("{top}/{b}"), &data[..])
                    .unwrap();
            }
            tb.finish().unwrap();
        }
        gz.finish().unwrap()
    }

    fn all_bins() -> Vec<&'static str> {
        toolchain_bins().collect()
    }

    /// A release dir laid out like GitHub's download URLs, plus a releases JSON.
    struct FakeRelease {
        root: TempDir,
    }

    impl FakeRelease {
        fn new() -> Self {
            FakeRelease {
                root: TempDir::new().unwrap(),
            }
        }
        fn publish(&self, ver: &str, tarball: &[u8], sums_override: Option<String>) {
            let d = self.root.path().join(format!("v{ver}"));
            fs::create_dir_all(&d).unwrap();
            let name = format!("crush-{ver}-{SUPPORTED_TARGET}.tar.gz");
            fs::write(d.join(&name), tarball).unwrap();
            let sums = sums_override
                .unwrap_or_else(|| format!("{}  {name}\n", hex(&Sha256::digest(tarball))));
            fs::write(d.join(SUMS_FILE), sums).unwrap();
            // releases list: every published version
            let mut list = Vec::new();
            for e in fs::read_dir(self.root.path()).unwrap() {
                let tag = e.unwrap().file_name().to_string_lossy().to_string();
                if let Some(ver) = tag.strip_prefix('v') {
                    list.push(serde_json::json!({
                        "tag_name": tag, "draft": false, "prerelease": true,
                        "assets": [{"name": format!("crush-{ver}-{SUPPORTED_TARGET}.tar.gz")}, {"name": SUMS_FILE}],
                    }));
                }
            }
            fs::write(
                self.root.path().join("releases.json"),
                serde_json::to_vec(&list).unwrap(),
            )
            .unwrap();
        }
        fn source(&self) -> Source {
            Source {
                releases_url: format!(
                    "file://{}",
                    self.root.path().join("releases.json").display()
                ),
                download_base: format!("file://{}", self.root.path().display()),
                token: None,
            }
        }
    }

    fn old_install(ver: &str) -> TempDir {
        let d = TempDir::new().unwrap();
        for b in all_bins() {
            let p = d.path().join(b);
            fs::write(&p, script(b, ver)).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        d
    }

    fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().to_string(),
                    fs::read(e.path()).unwrap_or_default(),
                )
            })
            .collect();
        v.sort();
        v
    }

    fn opts(dir: &Path) -> Options {
        Options {
            check: false,
            pin: None,
            dir: dir.to_path_buf(),
            force: false,
            quiet: false,
        }
    }

    fn run(o: &Options, rel: &FakeRelease, home: Option<&Path>) -> (Result<Outcome>, String) {
        let mut out = Vec::new();
        let r = run_update(
            o,
            &rel.source(),
            SUPPORTED_TARGET,
            home,
            &mut out,
            &mut |_| Ok(()),
        );
        (r, String::from_utf8(out).unwrap())
    }

    #[test]
    fn update_replaces_every_binary_and_leaves_no_staging() {
        let rel = FakeRelease::new();
        rel.publish("0.9.0", &make_tarball("0.9.0", &all_bins()), None);
        let dir = old_install("0.8.0");
        fs::write(dir.path().join("unrelated.txt"), "keep").unwrap();

        let (r, log) = run(&opts(dir.path()), &rel, None);
        assert_eq!(r.unwrap(), Outcome::Updated, "{log}");
        assert!(log.contains("updating crush 0.8.0 -> 0.9.0"), "{log}");
        assert_eq!(installed_version(dir.path()), Some(v("0.9.0")));
        for b in all_bins() {
            assert_eq!(
                fs::read(dir.path().join(b)).unwrap(),
                script(b, "0.9.0"),
                "{b}"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(dir.path().join(b))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o111, 0o111, "{b} must be executable");
            }
        }
        let names: Vec<_> = snapshot(dir.path()).into_iter().map(|(n, _)| n).collect();
        assert!(
            names.iter().all(|n| !n.starts_with(".crush-update-")),
            "{names:?}"
        );
        assert!(names.contains(&"unrelated.txt".to_string()));

        // A second run is a no-op.
        let (r, log) = run(&opts(dir.path()), &rel, None);
        assert_eq!(r.unwrap(), Outcome::UpToDate);
        assert!(log.contains("up to date"), "{log}");
    }

    #[test]
    fn checksum_mismatch_refuses_and_leaves_install_untouched() {
        let rel = FakeRelease::new();
        let tb = make_tarball("0.9.0", &all_bins());
        let name = format!("crush-0.9.0-{SUPPORTED_TARGET}.tar.gz");
        rel.publish("0.9.0", &tb, Some(format!("{}  {name}\n", "0".repeat(64))));
        let dir = old_install("0.8.0");
        let before = snapshot(dir.path());
        let (r, _) = run(&opts(dir.path()), &rel, None);
        let e = format!("{:#}", r.unwrap_err());
        assert!(e.contains("checksum mismatch"), "{e}");
        assert_eq!(
            snapshot(dir.path()),
            before,
            "install dir must be byte-identical"
        );
    }

    #[test]
    fn missing_sums_entry_refuses_and_leaves_install_untouched() {
        let rel = FakeRelease::new();
        rel.publish(
            "0.9.0",
            &make_tarball("0.9.0", &all_bins()),
            Some(format!("{}  other.tar.gz\n", "0".repeat(64))),
        );
        let dir = old_install("0.8.0");
        let before = snapshot(dir.path());
        let e = format!("{:#}", run(&opts(dir.path()), &rel, None).0.unwrap_err());
        assert!(e.contains("no entry"), "{e}");
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn tarball_missing_a_required_binary_refuses_before_touching_anything() {
        let rel = FakeRelease::new();
        let partial: Vec<_> = all_bins()
            .into_iter()
            .filter(|b| *b != "crush-run")
            .collect();
        rel.publish("0.9.0", &make_tarball("0.9.0", &partial), None);
        let dir = old_install("0.8.0");
        let before = snapshot(dir.path());
        let e = format!("{:#}", run(&opts(dir.path()), &rel, None).0.unwrap_err());
        assert!(e.contains("no 'crush-run'"), "{e}");
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn failure_midway_through_replace_rolls_back_to_the_old_binaries() {
        let rel = FakeRelease::new();
        rel.publish("0.9.0", &make_tarball("0.9.0", &all_bins()), None);
        let dir = old_install("0.8.0");
        let before = snapshot(dir.path());
        let mut renames = 0;
        let mut hook = |name: &str| {
            renames += 1;
            if renames == 4 {
                Err(io::Error::other(format!("injected failure before {name}")))
            } else {
                Ok(())
            }
        };
        let mut out = Vec::new();
        let r = run_update(
            &opts(dir.path()),
            &rel.source(),
            SUPPORTED_TARGET,
            None,
            &mut out,
            &mut hook,
        );
        let e = format!("{:#}", r.unwrap_err());
        assert!(
            e.contains("injected failure") && e.contains("rolled back"),
            "{e}"
        );
        assert_eq!(
            renames, 4,
            "three binaries were replaced before the failure"
        );
        assert_eq!(
            snapshot(dir.path()),
            before,
            "every binary is the old one again"
        );
        assert_eq!(installed_version(dir.path()), Some(v("0.8.0")));
    }

    #[test]
    fn replace_all_rollback_removes_binaries_that_did_not_exist_before() {
        let dir = TempDir::new().unwrap();
        let stage = TempDir::new_in(dir.path()).unwrap();
        fs::write(dir.path().join("a"), "old-a").unwrap();
        let mut staged = Vec::new();
        for n in ["a", "b", "c"] {
            let p = stage.path().join(n);
            fs::write(&p, format!("new-{n}")).unwrap();
            staged.push((n.to_string(), p));
        }
        let mut hook = |n: &str| {
            if n == "c" {
                Err(io::Error::other("boom"))
            } else {
                Ok(())
            }
        };
        assert!(replace_all(dir.path(), &staged, &stage.path().join("bk"), &mut hook).is_err());
        assert_eq!(fs::read_to_string(dir.path().join("a")).unwrap(), "old-a");
        assert!(
            !dir.path().join("b").exists(),
            "b was new; rollback removes it"
        );
        assert!(!dir.path().join("c").exists());
    }

    #[test]
    fn never_downgrades_unless_pinned() {
        let rel = FakeRelease::new();
        rel.publish("0.9.0", &make_tarball("0.9.0", &all_bins()), None);
        let dir = old_install("0.10.0");
        let (r, log) = run(&opts(dir.path()), &rel, None);
        assert_eq!(r.unwrap(), Outcome::UpToDate);
        assert!(log.contains("not downgrading"), "{log}");
        assert_eq!(installed_version(dir.path()), Some(v("0.10.0")));

        let pinned = Options {
            pin: Some(v("0.9.0")),
            ..opts(dir.path())
        };
        let (r, log) = run(&pinned, &rel, None);
        assert_eq!(r.unwrap(), Outcome::Updated, "{log}");
        assert!(log.contains("downgrading crush 0.10.0 -> 0.9.0"), "{log}");
        assert_eq!(installed_version(dir.path()), Some(v("0.9.0")));
    }

    #[test]
    fn latest_picks_highest_release_with_a_build_and_skips_semver_prereleases() {
        let rel = FakeRelease::new();
        for ver in ["0.9.0", "0.10.0", "0.11.0-rc.1"] {
            rel.publish(ver, &make_tarball(ver, &all_bins()), None);
        }
        assert_eq!(rel.source().latest(SUPPORTED_TARGET).unwrap(), v("0.10.0"));
        let e = rel
            .source()
            .latest("aarch64-unknown-linux-gnu")
            .unwrap_err()
            .to_string();
        assert!(e.contains("no release with a aarch64"), "{e}");
    }

    #[test]
    fn check_reports_and_never_writes() {
        let rel = FakeRelease::new();
        rel.publish("0.9.0", &make_tarball("0.9.0", &all_bins()), None);
        let dir = old_install("0.8.0");
        let before = snapshot(dir.path());
        let check = Options {
            check: true,
            ..opts(dir.path())
        };
        let (r, out) = run(&check, &rel, None);
        let o = r.unwrap();
        assert_eq!(o, Outcome::UpdateAvailable);
        assert_eq!(o.exit_code(), EXIT_UPDATE_AVAILABLE);
        assert!(
            out.contains("installed: 0.8.0") && out.contains("latest: 0.9.0"),
            "{out}"
        );
        assert!(out.contains("update available: 0.8.0 -> 0.9.0"), "{out}");
        assert_eq!(snapshot(dir.path()), before);

        let current = old_install("0.9.0");
        let (r, out) = run(
            &Options {
                check: true,
                ..opts(current.path())
            },
            &rel,
            None,
        );
        let o = r.unwrap();
        assert_eq!(
            (o.exit_code(), out.contains("up to date")),
            (0, true),
            "{out}"
        );
        assert_eq!(Outcome::Updated.exit_code(), 0);
    }

    fn crates2(root: &Path, bins: &[&str]) {
        let json = serde_json::json!({"installs": {
            "crush-lang-sdk 0.9.0 (registry+https://github.com/rust-lang/crates.io-index)":
                {"version_req": null, "bins": bins, "features": [], "all_features": false,
                 "no_default_features": false, "profile": "release", "target": "x86_64-unknown-linux-gnu",
                 "rustc": "rustc 1.95.0"}}});
        fs::write(
            root.join(".crates2.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn cargo_install_detection() {
        // $CARGO_HOME/bin holding a crush
        let home = TempDir::new().unwrap();
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        assert_eq!(
            cargo_managed(&bin, Some(home.path())),
            None,
            "no crush there yet"
        );
        fs::write(bin.join("crush"), "x").unwrap();
        assert!(cargo_managed(&bin, Some(home.path())).is_some());

        // cargo install --root R: R/.crates2.json + R/bin
        let root = TempDir::new().unwrap();
        let rbin = root.path().join("bin");
        fs::create_dir(&rbin).unwrap();
        crates2(root.path(), &["crush", "crushc"]);
        let why = cargo_managed(&rbin, None).unwrap();
        assert!(why.contains("crush-lang-sdk"), "{why}");

        // a ledger listing only unrelated binaries does not count
        let other = TempDir::new().unwrap();
        let obin = other.path().join("bin");
        fs::create_dir(&obin).unwrap();
        crates2(other.path(), &["ripgrep"]);
        assert_eq!(cargo_managed(&obin, None), None);

        // and a plain dir is not cargo's
        let plain = old_install("0.8.0");
        assert_eq!(cargo_managed(plain.path(), Some(home.path())), None);
    }

    #[test]
    fn cargo_managed_install_is_refused_unless_forced() {
        let rel = FakeRelease::new();
        rel.publish("0.9.0", &make_tarball("0.9.0", &all_bins()), None);
        let home = TempDir::new().unwrap();
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for b in all_bins() {
            fs::write(bin.join(b), script(b, "0.8.0")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(bin.join(b), fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let before = snapshot(&bin);
        let e = format!(
            "{:#}",
            run(&opts(&bin), &rel, Some(home.path())).0.unwrap_err()
        );
        assert!(
            e.contains("installed with cargo; run: cargo install crush-lang-sdk"),
            "{e}"
        );
        assert_eq!(snapshot(&bin), before);

        // --check still reports (read-only) and notes the cargo install
        let (r, out) = run(
            &Options {
                check: true,
                ..opts(&bin)
            },
            &rel,
            Some(home.path()),
        );
        assert_eq!(r.unwrap(), Outcome::UpdateAvailable);
        assert!(out.contains("installed with cargo"), "{out}");

        let forced = Options {
            force: true,
            ..opts(&bin)
        };
        assert_eq!(
            run(&forced, &rel, Some(home.path())).0.unwrap(),
            Outcome::Updated
        );
        assert_eq!(installed_version(&bin), Some(v("0.9.0")));
    }

    #[test]
    fn resolve_dir_prefers_explicit_then_managed_running_dir() {
        let explicit = PathBuf::from("/x/y");
        assert_eq!(resolve_dir(Some(explicit.clone()), None, None), explicit);

        // installer-managed: <prefix>/bin with <prefix>/.crush/install.json
        let prefix = TempDir::new().unwrap();
        let bin = prefix.path().join("bin");
        fs::create_dir(&bin).unwrap();
        InstallManifest {
            version: "0.8.0".into(),
            install_date: 0,
            prefix: prefix.path().to_path_buf(),
            bin_dir: bin.clone(),
            lib_dir: prefix.path().join("lib"),
            installed_binaries: vec![],
            modified_profiles: vec![],
        }
        .save(prefix.path())
        .unwrap();
        assert!(installer_managed(&bin));
        assert_eq!(resolve_dir(None, Some(bin.clone()), None), bin);

        // an unmanaged running dir (e.g. target/debug) falls back to ~/.crush/bin
        let plain = TempDir::new().unwrap();
        assert_eq!(
            resolve_dir(None, Some(plain.path().to_path_buf()), None),
            default_prefix().join("bin")
        );
    }

    #[test]
    fn update_refreshes_the_installer_manifest_version() {
        let rel = FakeRelease::new();
        rel.publish("0.9.0", &make_tarball("0.9.0", &all_bins()), None);
        let prefix = old_install("0.8.0"); // binaries go in prefix/bin below
        let bin = prefix.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for b in all_bins() {
            fs::rename(prefix.path().join(b), bin.join(b)).unwrap();
        }
        InstallManifest {
            version: "0.8.0".into(),
            install_date: 0,
            prefix: prefix.path().to_path_buf(),
            bin_dir: bin.clone(),
            lib_dir: prefix.path().join("lib"),
            installed_binaries: vec!["crush".into()],
            modified_profiles: vec![],
        }
        .save(prefix.path())
        .unwrap();
        assert_eq!(run(&opts(&bin), &rel, None).0.unwrap(), Outcome::Updated);
        let m = InstallManifest::load(prefix.path()).unwrap().unwrap();
        assert_eq!(m.version, "0.9.0");
        assert!(m.installed_binaries.contains(&"crush-run".to_string()));
    }
}
