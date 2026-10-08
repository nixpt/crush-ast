//! Common host-provided capabilities for the CRUSH runtime.
//!
//! These capabilities extend the portable CVM1 instruction set with
//! filesystem, environment, and time operations. They are registered
//! explicitly by the host via [`HostCaps`](crush_vm::HostCaps).

use crush_index::CrushIndex;
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "stdlib")]
use std::sync::Mutex;

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// Builder for a standard set of host capabilities.
#[derive(Default)]
pub struct HostCapsBuilder {
    fs: bool,
    polyglot: Vec<&'static str>,
    env: bool,
    time: bool,
    bus: bool,
    task: bool,
    akg: bool,
    process: bool,
    crypto: bool,
    #[cfg(feature = "graphics")]
    graphics: bool,
    #[cfg(feature = "net")]
    net: bool,
    #[cfg(feature = "net")]
    net_max_response_bytes: usize,
    #[cfg(feature = "db")]
    db_path: Option<String>,
    #[cfg(feature = "stdlib")]
    stdlib: bool,
    fs_root: Option<String>,
    env_vars: HashMap<String, String>,
    /// Pre-built codebase index for `codebase.*` caps.
    codebase_index: Option<Arc<CrushIndex>>,
    /// CRUSH-32: register the `ai_native.*` capability surface (default off).
    ai_native: bool,
}

impl HostCapsBuilder {
    /// Create a new builder with all capabilities disabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant polyglot execution for the given languages (canonical: "python", "javascript",
    /// "bash"). Each becomes a `polyglot.<lang>` gate in the registry. Without this, @lang blocks
    /// refuse to spawn — polyglot is NOT ambient.
    pub fn polyglot(mut self, langs: &[&'static str]) -> Self {
        self.polyglot = langs.to_vec();
        self
    }

    /// Enable filesystem capabilities: `fs.read`, `fs.write`, `fs.exists`,
    /// `fs.list`, the coreutils `fs.ls/cat/pwd/cd/mkdir/rm/cp/mv/touch/find`,
    /// and the `text.*` file tools — all confined to [`fs_root`](Self::fs_root).
    pub fn fs(mut self, enable: bool) -> Self {
        self.fs = enable;
        self
    }

    /// Restrict filesystem access to paths under `root`.
    pub fn fs_root(mut self, root: impl Into<String>) -> Self {
        self.fs_root = Some(root.into());
        self
    }

    /// Enable environment variable access (`env.get`, `env.all`, `env.home_dir`).
    /// The grant exposes the host process environment, with any
    /// [`with_env_var`](Self::with_env_var) values layered on top.
    pub fn env(mut self, enable: bool) -> Self {
        self.env = enable;
        self
    }

    /// Inject a specific environment variable value.
    pub fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Enable time capabilities (`time.now`, `time.now_ms`, `time.now_iso`,
    /// `time.elapsed`, `time.sleep`, and its alias `async.sleep`).
    pub fn time(mut self, enable: bool) -> Self {
        self.time = enable;
        self
    }

    /// Enable message-bus capabilities (`message_bus.publish`, `message_bus.subscribe`, `message_bus.recv`).
    pub fn bus(mut self, enable: bool) -> Self {
        self.bus = enable;
        self
    }

    /// Enable task-management capabilities (`task.start`, `task.stop`, `task.list`).
    pub fn task(mut self, enable: bool) -> Self {
        self.task = enable;
        self
    }

    /// Enable knowledge-graph capabilities (`akg.write`, `akg.read`, `akg.search`).
    pub fn akg(mut self, enable: bool) -> Self {
        self.akg = enable;
        self
    }

    /// Enable process capabilities (`process.exec`).
    pub fn process(mut self, enable: bool) -> Self {
        self.process = enable;
        self
    }

    /// Enable cryptography capabilities (`crypto.sha256`, `crypto.random`).
    pub fn crypto(mut self, enable: bool) -> Self {
        self.crypto = enable;
        self
    }

    /// Enable graphics capabilities (`graphics.canvas`, `graphics.rect`,
    /// `graphics.circle`, `graphics.text`, `graphics.to_svg`).
    #[cfg(feature = "graphics")]
    pub fn graphics(mut self, enable: bool) -> Self {
        self.graphics = enable;
        self
    }

    /// Enable network capabilities (`net.http_get`, `net.http_post`).
    #[cfg(feature = "net")]
    pub fn net(mut self, enable: bool) -> Self {
        self.net = enable;
        self
    }

    /// Set the maximum HTTP response size in bytes.
    #[cfg(feature = "net")]
    pub fn net_max_response_bytes(mut self, n: usize) -> Self {
        self.net_max_response_bytes = n;
        self
    }

    /// Enable database capabilities (`db.query`, `db.execute`) on the given path.
    #[cfg(feature = "db")]
    pub fn db(mut self, path: impl Into<String>) -> Self {
        self.db_path = Some(path.into());
        self
    }

    /// Enable standard library capabilities (str.*, math.*, etc.).
    #[cfg(feature = "stdlib")]
    pub fn stdlib(mut self, enable: bool) -> Self {
        self.stdlib = enable;
        self
    }

    /// Inject a pre-built `CrushIndex` to enable `codebase.*` capabilities.
    ///
    /// Build the index by calling `CrushIndex::add_program` for each compiled
    /// Crush module, then pass it here before calling `build()`.
    pub fn codebase(mut self, index: CrushIndex) -> Self {
        self.codebase_index = Some(Arc::new(index));
        self
    }

    /// Build the [`HostCaps`] registry.
    /// **CRUSH-32**: register the `ai_native.*` capability surface. All 10
    /// gates (`ai_native.query`, `ai_native.synthesize`,
    /// `ai_native.agent_delegation`, `ai_native.semantic_match`,
    /// `ai_native.learning_loop`, `ai_native.context_aware`,
    /// `ai_native.toolchain`, `ai_native.goal_declaration`,
    /// `ai_native.progress_update`, `ai_native.knowledge_sharing`) are
    /// registered as deterministic stubs returning
    /// `Value::Map({ok: true, kind: "<name>", echo: <args>})`. Real AI
    /// backends will replace these stubs in later milestones, but the
    /// surface shape and gate names are stable.
    pub fn ai_native(mut self, enable: bool) -> Self {
        self.ai_native = enable;
        self
    }

    pub fn build(self) -> HostCaps {
        let mut caps = HostCaps::new();
        caps.register(Box::new(crush_caison::vm_cap::CaisonParseCap::new()));
        // Deprecated pre-rename name; removed in 0.4 (CRUSH-149).
        caps.register(Box::new(crush_caison::vm_cap::CaisonParseCap::deprecated_alias()));
        caps.grant_polyglot(&self.polyglot);
        if self.fs {
            // One sandbox, so `fs.cd` moves the working directory every
            // file cap in this registry resolves against.
            let fs = FsSandbox::new(self.fs_root.as_deref().unwrap_or("."));
            caps.register(Box::new(FsReadCap { fs: fs.clone() }));
            caps.register(Box::new(FsWriteCap { fs: fs.clone() }));
            caps.register(Box::new(FsExistsCap { fs: fs.clone() }));
            caps.register(Box::new(FsListCap { fs: fs.clone() }));
            crate::fs_tools::register(&mut caps, &fs);
            crate::text_tools::register(&mut caps, &fs);
        }
        if self.env {
            caps.register(Box::new(EnvAllCap {
                overrides: self.env_vars.clone(),
            }));
            caps.register(Box::new(EnvHomeDirCap {
                overrides: self.env_vars.clone(),
            }));
            caps.register(Box::new(EnvGetCap::new(self.env_vars)));
        }
        if self.time {
            caps.register(Box::new(TimeNowCap));
            caps.register(Box::new(TimeNowMsCap));
            caps.register(Box::new(TimeNowIsoCap));
            caps.register(Box::new(TimeElapsedCap));
            caps.register(Box::new(TimeSleepCap));
            caps.register(Box::new(AsyncSleepCap));
        }
        if self.bus {
            crate::bus::register(&mut caps);
        }
        if self.task {
            crate::task::register(&mut caps);
        }
        if self.akg {
            crate::akg::register(&mut caps);
        }
        if self.process {
            caps.register(Box::new(ProcessExecCap));
        }
        if self.crypto {
            caps.register(Box::new(CryptoSha256Cap));
            caps.register(Box::new(CryptoRandomCap));
        }
        #[cfg(feature = "graphics")]
        if self.graphics {
            crate::graphics::register(&mut caps);
        }
        #[cfg(feature = "net")]
        if self.net {
            crate::net::register(&mut caps, self.net_max_response_bytes.max(1));
        }
        #[cfg(feature = "db")]
        if let Some(path) = self.db_path {
            if let Err(e) = crate::db::register(&mut caps, &path) {
                eprintln!("crush-lang-sdk: failed to register db capabilities: {e}");
            }
        }
        // The stdlib is pure: declared with no effects as a family.
        #[cfg(feature = "stdlib")]
        if self.stdlib {
            let mut stdlib = HostCaps::new();
            crate::stdlib::register_with_rng(
                &mut stdlib,
                Arc::new(Mutex::new(crate::stdlib::RngState::new(0))),
            );
            crate::effects::register_all(&mut caps, stdlib, &[]);
        }
        if let Some(idx) = self.codebase_index {
            crate::codebase::register(&mut caps, idx);
        }
        if self.ai_native {
            crate::ai_native::register(&mut caps);
        }
        crate::effects::declare(caps)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Filesystem helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve `path` against `cwd` (relative to `root`) and confine it to
/// `root`. With `follow` false the last component is not resolved through a
/// symlink, so `fs.rm`/`fs.mv` act on a link rather than on what it names.
fn resolve_in(
    root: &str,
    cwd: &std::path::Path,
    path: &Value,
    follow: bool,
) -> Result<std::path::PathBuf, String> {
    use std::path::{Component, Path};

    let s = crate::caps::value_as_text(path);
    let p = Path::new(&s);
    if p.is_absolute() {
        return Err(format!("absolute paths are not allowed: {s}"));
    }
    let escapes = || format!("path escapes sandbox root: {s}");
    let root = Path::new(root);
    let root_canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    // Resolve `.` / `..` lexically first. A path that does not exist yet
    // (an `fs.write` target) cannot be canonicalized, and `Path::starts_with`
    // compares components, so `<root>/../x` used to pass the check below and
    // let `fs.write("../x", ..)` write outside the sandbox.
    let mut relative = cwd.to_path_buf();
    for component in p.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => relative.push(part),
            Component::ParentDir => {
                if !relative.pop() {
                    return Err(escapes());
                }
            }
            Component::RootDir | Component::Prefix(_) => return Err(escapes()),
        }
    }

    // Not following the last component: confine its parent, keep the name.
    if !follow && let Some(name) = relative.file_name().map(|n| n.to_os_string()) {
        let parent = relative.parent().unwrap_or(Path::new("")).to_path_buf();
        let dir = confine(&root_canonical, &parent).ok_or_else(escapes)?;
        return Ok(dir.join(name));
    }
    confine(&root_canonical, &relative).ok_or_else(escapes)
}

/// Join `relative` onto the canonical root, resolving symlinks through the
/// deepest ancestor that exists, so a symlink inside the root cannot point a
/// new file outside it either. `None` if the result leaves the root.
fn confine(
    root_canonical: &std::path::Path,
    relative: &std::path::Path,
) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let joined = root_canonical.join(relative);
    let mut existing = joined.as_path();
    let mut rest = Vec::new();
    let resolved = loop {
        if let Ok(canonical) = existing.canonicalize() {
            break rest
                .iter()
                .rev()
                .fold(canonical, |acc: PathBuf, part| acc.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => break joined.clone(),
        }
    };
    resolved.starts_with(root_canonical).then_some(resolved)
}

/// The `--fs` sandbox one registry's file caps share: the root, plus the
/// working directory `fs.cd` moves (CRUSH-151, decision C-5). The working
/// directory is per-registry — local to the VM the registry serves — always
/// inside the root, and never the host process's cwd.
#[derive(Clone)]
pub(crate) struct FsSandbox {
    root: String,
    /// Relative to the canonical root; empty = the root itself.
    cwd: Arc<std::sync::Mutex<std::path::PathBuf>>,
}

impl FsSandbox {
    pub(crate) fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
            cwd: Arc::default(),
        }
    }

    fn cwd(&self) -> std::path::PathBuf {
        self.cwd.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn root_canonical(&self) -> std::path::PathBuf {
        let root = std::path::Path::new(&self.root);
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
    }

    /// Resolve a program path against the working directory, inside the root.
    pub(crate) fn resolve(&self, path: &Value) -> Result<std::path::PathBuf, String> {
        resolve_in(&self.root, &self.cwd(), path, true)
    }

    /// Like [`resolve`](Self::resolve), but a symlink in the last component
    /// is the target itself rather than what it points to.
    pub(crate) fn resolve_nofollow(&self, path: &Value) -> Result<std::path::PathBuf, String> {
        resolve_in(&self.root, &self.cwd(), path, false)
    }

    /// `fs.cd`: move the working directory to an existing directory.
    pub(crate) fn cd(&self, path: &Value) -> Result<(), String> {
        let target = self.resolve(path)?;
        if !target.is_dir() {
            return Err(format!("fs.cd {}: not a directory", self.display(&target)));
        }
        let relative = target
            .strip_prefix(self.root_canonical())
            .map_err(|_| format!("fs.cd {}: outside the sandbox", self.display(&target)))?
            .to_path_buf();
        *self.cwd.lock().unwrap_or_else(|e| e.into_inner()) = relative;
        Ok(())
    }

    /// `fs.pwd`: the working directory relative to the root, `.` at the root.
    pub(crate) fn pwd(&self) -> String {
        let cwd = self.cwd();
        if cwd.as_os_str().is_empty() {
            ".".to_string()
        } else {
            cwd.to_string_lossy().into_owned()
        }
    }

    /// A resolved path as the program sees it (relative to the root), so
    /// error messages never print the host's absolute path.
    pub(crate) fn display(&self, path: &std::path::Path) -> String {
        match path.strip_prefix(self.root_canonical()) {
            Ok(p) if p.as_os_str().is_empty() => ".".to_string(),
            Ok(p) => p.to_string_lossy().into_owned(),
            Err(_) => path.to_string_lossy().into_owned(),
        }
    }

    /// Refuse to remove or move the root, the working directory, or any
    /// directory containing it — the sandbox would lose its footing.
    pub(crate) fn refuse_working_dir(
        &self,
        cap: &str,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let cwd = self.root_canonical().join(self.cwd());
        if cwd.starts_with(path) {
            return Err(format!(
                "{cap} {}: refusing to remove or move the sandbox root or the working directory",
                self.display(path)
            ));
        }
        Ok(())
    }
}

pub struct FsReadCap {
    fs: FsSandbox,
}

impl FsReadCap {
    /// A stand-alone cap confined to `root`, with its own working directory
    /// (the builder shares one sandbox across all file caps instead).
    pub fn new(root: &str) -> Self {
        Self {
            fs: FsSandbox::new(root),
        }
    }
}

impl HostCap for FsReadCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "fs.read".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let path = self.fs.resolve(&args[0])?;
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("fs.read {}: {e}", self.fs.display(&path)))?;
        Ok(Some(Value::Str(data)))
    }
}

pub struct FsWriteCap {
    fs: FsSandbox,
}

impl FsWriteCap {
    /// A stand-alone cap confined to `root`, with its own working directory
    /// (the builder shares one sandbox across all file caps instead).
    pub fn new(root: &str) -> Self {
        Self {
            fs: FsSandbox::new(root),
        }
    }
}

impl HostCap for FsWriteCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "fs.write".to_string(),
            argc: Some(2),
            returns: false,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let path = self.fs.resolve(&args[0])?;
        let data = crate::caps::value_as_text(&args[1]);
        std::fs::write(&path, data)
            .map_err(|e| format!("fs.write {}: {e}", self.fs.display(&path)))?;
        Ok(None)
    }
}

pub struct FsExistsCap {
    fs: FsSandbox,
}

impl FsExistsCap {
    /// A stand-alone cap confined to `root`, with its own working directory
    /// (the builder shares one sandbox across all file caps instead).
    pub fn new(root: &str) -> Self {
        Self {
            fs: FsSandbox::new(root),
        }
    }
}

impl HostCap for FsExistsCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "fs.exists".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let path = self.fs.resolve(&args[0])?;
        Ok(Some(Value::Int(if path.exists() { 1 } else { 0 })))
    }
}

pub struct FsListCap {
    fs: FsSandbox,
}

impl FsListCap {
    /// A stand-alone cap confined to `root`, with its own working directory
    /// (the builder shares one sandbox across all file caps instead).
    pub fn new(root: &str) -> Self {
        Self {
            fs: FsSandbox::new(root),
        }
    }
}

impl HostCap for FsListCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "fs.list".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let path = self.fs.resolve(&args[0])?;
        crate::fs_tools::list_dir("fs.list", &path, &self.fs).map(Some)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Environment helpers
// ─────────────────────────────────────────────────────────────────────────────

pub struct EnvGetCap {
    overrides: HashMap<String, String>,
}

impl EnvGetCap {
    pub fn new(overrides: HashMap<String, String>) -> Self {
        Self { overrides }
    }
}

impl HostCap for EnvGetCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "env.get".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let key = crate::caps::value_as_text(&args[0]);
        if let Some(v) = self.overrides.get(&key) {
            return Ok(Some(Value::Str(v.clone())));
        }
        match std::env::var(&key) {
            Ok(v) => Ok(Some(Value::Str(v))),
            Err(_) => Ok(Some(Value::Null)),
        }
    }
}

/// `env.all()` — every variable the `--env` grant exposes, as a map: the
/// host environment with the builder's injected values on top (CRUSH-153).
/// Variables whose name or value is not valid Unicode are skipped.
pub struct EnvAllCap {
    overrides: HashMap<String, String>,
}

impl HostCap for EnvAllCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "env.all".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let mut all: HashMap<String, Value> = std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, Value::Str(v.into_string().ok()?))))
            .collect();
        for (k, v) in &self.overrides {
            all.insert(k.clone(), Value::Str(v.clone()));
        }
        Ok(Some(Value::new_map(all)))
    }
}

/// `env.home_dir()` — the user's home directory (`HOME`, or `USERPROFILE` on
/// Windows) as the `--env` grant sees it, or null when unset.
pub struct EnvHomeDirCap {
    overrides: HashMap<String, String>,
}

impl HostCap for EnvHomeDirCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "env.home_dir".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let home = self
            .overrides
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok());
        Ok(Some(home.map_or(Value::Null, Value::Str)))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Time helpers
// ─────────────────────────────────────────────────────────────────────────────

pub struct TimeNowCap;

impl HostCap for TimeNowCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "time.now".to_string(),
            argc: Some(0),
            returns: true,
        }
    }

    fn call(&self, _args: Vec<Value>) -> Result<Option<Value>, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        Ok(Some(Value::Int(now.as_secs() as i64)))
    }
}

// The rest of the clock family, ported from exosphere's stdlib `time_cap.rs`
// (W10 / CRUSH-122). nanovm's `time.now` returned milliseconds; this crate's
// `time.now` (above) has always returned seconds, so the millisecond clock is
// `time.now_ms` and `time.elapsed` / `time.sleep` work in milliseconds, as in
// nanovm. The pure half (`time.format` / `time.parse`) is in the stdlib.

fn now_ms() -> Result<i64, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    i64::try_from(now.as_millis()).map_err(|e| e.to_string())
}

macro_rules! time_cap {
    ($name:ident, $full:expr, $argc:expr, $body:expr) => {
        pub struct $name;
        impl HostCap for $name {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: $full.to_string(),
                    argc: Some($argc),
                    returns: true,
                }
            }
            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                #[allow(clippy::redundant_closure_call)]
                ($body)(&args)
            }
        }
    };
}

time_cap!(TimeNowMsCap, "time.now_ms", 0, |_args: &[Value]| {
    Ok(Some(Value::Int(now_ms()?)))
});

time_cap!(TimeNowIsoCap, "time.now_iso", 0, |_args: &[Value]| {
    Ok(Some(Value::Str(chrono::Utc::now().to_rfc3339())))
});

// Milliseconds since `start_ms` (a `time.now_ms` value).
time_cap!(TimeElapsedCap, "time.elapsed", 1, |args: &[Value]| {
    match &args[0] {
        Value::Int(start) => Ok(Some(Value::Int(now_ms()? - start))),
        other => Err(format!("time.elapsed: expected int milliseconds, got {other}")),
    }
});

/// `time.sleep(ms)` — blocks the calling thread (nanovm's `time.sleep` and
/// `async.sleep` were this same synchronous sleep under two names). A sleep
/// longer than the VM's wall-time quota stops at the quota and reports
/// `CapTimeout` rather than hanging the program.
/// `time.sleep(ms)` — block for `ms` milliseconds (`--time`).
pub struct TimeSleepCap;

/// `async.sleep(ms)` — the exosphere / nanovm name for the same blocking
/// sleep (`--time`, CRUSH-152). It does not yield to the scheduler; both
/// names run [`sleep_ms`].
pub struct AsyncSleepCap;

/// The one sleep implementation behind `time.sleep` and `async.sleep`: block
/// for the requested milliseconds, or — when the VM's wall-clock deadline is
/// shorter — sleep until the deadline and report a timeout.
fn sleep_ms(
    cap: &str,
    args: &[Value],
    deadline_ms: Option<u64>,
) -> Result<Option<Value>, crush_vm::host::HostCapError> {
    let ms = match args.first() {
        Some(Value::Int(ms)) if *ms >= 0 => *ms as u64,
        other => {
            return Err(format!(
                "{cap}: expected non-negative int milliseconds, got {}",
                other.map_or("nothing".to_string(), |v| v.to_string())
            )
            .into());
        }
    };
    if let Some(deadline) = deadline_ms
        && ms > deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(deadline));
        return Err(crush_vm::host::HostCapError::Timeout);
    }
    std::thread::sleep(std::time::Duration::from_millis(ms));
    Ok(Some(Value::Null))
}

macro_rules! sleep_cap {
    ($ty:ident, $name:expr) => {
        impl HostCap for $ty {
            fn spec(&self) -> HostCapSpec {
                HostCapSpec {
                    name: $name.to_string(),
                    argc: Some(1),
                    returns: true,
                }
            }

            fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
                // No deadline, so the only error is a bad argument.
                sleep_ms($name, &args, None).map_err(|e| match e {
                    crush_vm::host::HostCapError::Message(m) => m,
                    crush_vm::host::HostCapError::Timeout => format!("{}: timed out", $name),
                })
            }

            fn call_with_deadline(
                &self,
                args: Vec<Value>,
                deadline_ms: u64,
            ) -> Result<Option<Value>, crush_vm::host::HostCapError> {
                sleep_ms($name, &args, Some(deadline_ms))
            }
        }
    };
}

sleep_cap!(TimeSleepCap, "time.sleep");
sleep_cap!(AsyncSleepCap, "async.sleep");

// ─────────────────────────────────────────────────────────────────────────────
// Process helpers
// ─────────────────────────────────────────────────────────────────────────────

pub struct ProcessExecCap;

impl HostCap for ProcessExecCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "process.exec".to_string(),
            argc: Some(2),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let cmd = crate::caps::value_as_text(&args[0]);
        let exec_args: Vec<String> = match &args[1] {
            Value::Array(a) => a.borrow().iter().map(crate::caps::value_as_text).collect(),
            v => vec![crate::caps::value_as_text(v)],
        };

        let output = std::process::Command::new(&cmd)
            .args(&exec_args)
            .output()
            .map_err(|e| format!("process.exec {cmd}: {e}"))?;

        let result = serde_json::json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "exit_code": output.status.code().unwrap_or(-1),
        });
        Ok(Some(Value::Str(result.to_string())))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Crypto helpers
// ─────────────────────────────────────────────────────────────────────────────

pub struct CryptoSha256Cap;

impl HostCap for CryptoSha256Cap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "crypto.sha256".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        use sha2::{Digest, Sha256};
        let data = crate::caps::value_as_text(&args[0]);
        let hash = Sha256::digest(data.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Some(Value::Str(hex)))
    }
}

pub struct CryptoRandomCap;

impl HostCap for CryptoRandomCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "crypto.random".to_string(),
            argc: Some(1),
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        let n = match &args[0] {
            Value::Int(i) => *i,
            v => crate::caps::value_as_text(v)
                .parse::<i64>()
                .map_err(|e| format!("crypto.random: invalid count: {e}"))?,
        };
        if n < 0 {
            return Err("crypto.random: count must be non-negative".to_string());
        }
        if n > 4096 {
            return Err("crypto.random: count exceeds 4096 byte limit".to_string());
        }
        let mut buf = vec![0u8; n as usize];
        use rand::Rng;
        rand::thread_rng().fill(&mut buf[..]);
        Ok(Some(Value::Str(base64::Engine::encode(
            &base64::engine::GeneralPurpose::new(
                &base64::alphabet::STANDARD,
                base64::engine::GeneralPurposeConfig::default(),
            ),
            &buf,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_registers_caps() {
        let caps = HostCapsBuilder::new()
            .fs(true)
            .env(true)
            .time(true)
            .process(true)
            .crypto(true)
            .build();

        assert!(caps.get("fs.read").is_some());
        assert!(caps.get("env.get").is_some());
        assert!(caps.get("time.now").is_some());
        assert!(caps.get("time.now_ms").is_some());
        assert!(caps.get("time.sleep").is_some());
        assert!(caps.get("text.wc").is_some());
        assert!(caps.get("process.exec").is_some());
        assert!(caps.get("crypto.sha256").is_some());
        assert!(caps.get("crypto.random").is_some());
        assert!(caps.get("missing").is_none());
    }

    #[test]
    fn sandbox_rejects_parent_escapes_for_paths_that_do_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let root_str = root.to_str().unwrap();
        let write = FsWriteCap::new(root_str);
        let text = |s: &str| Value::Str(s.to_string());

        for escape in ["../escaped.txt", "a/../../escaped.txt", "./../escaped.txt"] {
            let err = write.call(vec![text(escape), text("x")]).unwrap_err();
            assert!(err.contains("escapes sandbox"), "{escape}: {err}");
        }
        assert!(!dir.path().join("escaped.txt").exists());

        // new files inside the root still work, including via `..` that stays inside
        write.call(vec![text("new.txt"), text("ok")]).unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        write.call(vec![text("sub/../new2.txt"), text("ok")]).unwrap();
        assert!(root.join("new.txt").exists() && root.join("new2.txt").exists());
    }

    #[test]
    fn sandbox_follows_symlinks_for_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        let write = FsWriteCap::new(root.to_str().unwrap());
        let err = write
            .call(vec![Value::Str("link/new.txt".into()), Value::Str("x".into())])
            .unwrap_err();
        assert!(err.contains("escapes sandbox"), "{err}");
        assert!(!outside.join("new.txt").exists());
    }

    #[test]
    fn time_sleep_respects_the_wall_time_deadline() {
        use crush_vm::host::HostCapError;
        let cap = TimeSleepCap;
        assert!(matches!(
            cap.call_with_deadline(vec![Value::Int(1)], 1_000),
            Ok(Some(Value::Null))
        ));
        assert!(matches!(
            cap.call_with_deadline(vec![Value::Int(60_000)], 5),
            Err(HostCapError::Timeout)
        ));
        assert!(cap.call(vec![Value::Int(-1)]).is_err());
    }

    #[test]
    fn env_all_and_home_dir_follow_the_env_grant() {
        let caps = HostCapsBuilder::new()
            .env(true)
            .with_env_var("CRUSH_153_INJECTED", "yes")
            .with_env_var("HOME", "/sandbox/home")
            .build();
        let Some(Value::Map(all)) = caps.get("env.all").unwrap().call(vec![]).unwrap() else {
            panic!("env.all must return a map");
        };
        let all = all.borrow();
        assert_eq!(all["CRUSH_153_INJECTED"], Value::Str("yes".into()));
        assert_eq!(all["HOME"], Value::Str("/sandbox/home".into()));
        if let Ok(path) = std::env::var("PATH") {
            assert_eq!(all["PATH"], Value::Str(path));
        }
        assert_eq!(
            caps.get("env.home_dir").unwrap().call(vec![]).unwrap(),
            Some(Value::Str("/sandbox/home".into()))
        );

        let ungranted = HostCapsBuilder::new().stdlib(true).time(true).build();
        for cap in ["env.get", "env.all", "env.home_dir"] {
            assert!(ungranted.get(cap).is_none(), "{cap} without --env");
        }
    }

    // Source → VM for one env cap.
    #[test]
    fn env_home_dir_through_the_source_pipeline() {
        let prog = crate::compile::compile_crush_source("io.print(env.home_dir())\n").unwrap();
        let caps = HostCapsBuilder::new()
            .env(true)
            .with_env_var("HOME", "/sandbox/home")
            .build();
        let quotas = crush_vm::Quotas::default();
        let result = crush_vm::run_with_caps(&prog, &quotas, Some(&caps)).unwrap();
        assert_eq!(result.output, "/sandbox/home\n");
        let refused = crush_vm::run_with_caps(&prog, &quotas, None).unwrap_err();
        assert!(refused.to_string().contains("env.home_dir"), "{refused}");
    }

    #[test]
    fn async_sleep_is_time_sleep_under_another_name() {
        use crush_vm::host::HostCapError;
        let cap = AsyncSleepCap;
        assert_eq!(cap.spec().name, "async.sleep");
        assert_eq!(cap.spec().argc, TimeSleepCap.spec().argc);
        assert!(matches!(
            cap.call_with_deadline(vec![Value::Int(1)], 1_000),
            Ok(Some(Value::Null))
        ));
        assert!(matches!(
            cap.call_with_deadline(vec![Value::Int(60_000)], 5),
            Err(HostCapError::Timeout)
        ));
        let err = cap.call(vec![Value::Int(-1)]).unwrap_err();
        assert!(err.starts_with("async.sleep:"), "{err}");
    }

    #[test]
    fn sleep_caps_need_the_time_grant() {
        let caps = HostCapsBuilder::new().stdlib(true).fs(true).build();
        assert!(caps.get("time.sleep").is_none());
        assert!(caps.get("async.sleep").is_none());
        let caps = HostCapsBuilder::new().time(true).build();
        assert!(caps.get("async.sleep").is_some());
    }

    // CRUSH-152: `await async.sleep(..)` from source, past the VM's
    // wall-clock limit, is a named CapTimeout — and returns at the limit,
    // not after the requested sleep.
    #[test]
    fn async_sleep_past_the_wall_clock_limit_is_a_cap_timeout() {
        let prog =
            crate::compile::compile_crush_source("await async.sleep(60000)\n").expect("compile");
        let caps = HostCapsBuilder::new().time(true).build();
        let quotas = crush_vm::Quotas {
            max_wall_time_ms: 100,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let err = crush_vm::run_with_caps(&prog, &quotas, Some(&caps)).unwrap_err();
        assert!(
            matches!(err, crush_vm::VmError::CapTimeout { .. }),
            "{err:?}"
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(5));

        let ok = crate::compile::compile_crush_source("await async.sleep(1)\nio.print(\"woke\")\n")
            .expect("compile");
        let result = crush_vm::run_with_caps(&ok, &quotas, Some(&caps)).unwrap();
        assert_eq!(result.output, "woke\n");
    }

    #[test]
    fn time_elapsed_counts_from_now_ms() {
        let now = TimeNowMsCap.call(vec![]).unwrap().unwrap();
        let Some(Value::Int(elapsed)) = TimeElapsedCap.call(vec![now]).unwrap() else {
            panic!("expected int");
        };
        assert!((0..60_000).contains(&elapsed));
    }

    #[test]
    fn fs_read_host_cap() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "hello fs").unwrap();
        let dir = tmp.path().parent().unwrap().to_str().unwrap();

        let cap = FsReadCap::new(dir);
        let result = cap.call(vec![Value::Str(
            tmp.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        )]);
        assert_eq!(result.unwrap(), Some(Value::Str("hello fs".to_string())));
    }

    #[test]
    fn fs_blocks_absolute_path() {
        let cap = FsReadCap::new("/tmp");
        let result = cap.call(vec![Value::Str("/etc/passwd".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn env_cap_uses_override() {
        let mut map = HashMap::new();
        map.insert("FOO".to_string(), "bar".to_string());
        let cap = EnvGetCap::new(map);
        let result = cap.call(vec![Value::Str("FOO".to_string())]).unwrap();
        assert_eq!(result, Some(Value::Str("bar".to_string())));
    }

    #[test]
    fn process_exec_runs_echo() {
        let cap = ProcessExecCap;
        let result = cap
            .call(vec![
                Value::Str("echo".to_string()),
                Value::new_array(vec![Value::Str("hello".to_string())]),
            ])
            .unwrap();
        let text = crate::caps::value_as_text(&result.unwrap());
        assert!(text.contains("\"stdout\":\"hello"));
        assert!(text.contains("\"exit_code\":0"));
    }

    #[test]
    fn crypto_sha256_matches_known_vector() {
        let cap = CryptoSha256Cap;
        let result = cap.call(vec![Value::Str("hello".to_string())]).unwrap();
        let hex = crate::caps::value_as_text(&result.unwrap());
        assert_eq!(
            hex,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn crypto_random_returns_requested_bytes() {
        let cap = CryptoRandomCap;
        let result = cap.call(vec![Value::Int(16)]).unwrap();
        let b64 = crate::caps::value_as_text(&result.unwrap());
        assert_eq!(
            base64::Engine::decode(
                &base64::engine::GeneralPurpose::new(
                    &base64::alphabet::STANDARD,
                    base64::engine::GeneralPurposeConfig::default(),
                ),
                &b64,
            )
            .unwrap()
            .len(),
            16
        );
    }
}


