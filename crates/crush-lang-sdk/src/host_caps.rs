//! Common host-provided capabilities for the CRUSH runtime.
//!
//! These capabilities extend the portable CVM1 instruction set with
//! filesystem, environment, and time operations. They are registered
//! explicitly by the host via [`HostCaps`](crush_vm::HostCaps).

use std::collections::HashMap;

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

/// Builder for a standard set of host capabilities.
#[derive(Debug, Default)]
pub struct HostCapsBuilder {
    fs: bool,
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
}

impl HostCapsBuilder {
    /// Create a new builder with all capabilities disabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable filesystem capabilities (`fs.read`, `fs.write`, `fs.exists`, `fs.list`).
    pub fn fs(mut self, enable: bool) -> Self {
        self.fs = enable;
        self
    }

    /// Restrict filesystem access to paths under `root`.
    pub fn fs_root(mut self, root: impl Into<String>) -> Self {
        self.fs_root = Some(root.into());
        self
    }

    /// Enable environment variable access (`env.get`).
    pub fn env(mut self, enable: bool) -> Self {
        self.env = enable;
        self
    }

    /// Inject a specific environment variable value.
    pub fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Enable time capabilities (`time.now`).
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

    /// Build the [`HostCaps`] registry.
    pub fn build(self) -> HostCaps {
        let mut caps = HostCaps::new();
        if self.fs {
            let root = self.fs_root.unwrap_or_else(|| ".".to_string());
            caps.register(Box::new(FsReadCap::new(&root)));
            caps.register(Box::new(FsWriteCap::new(&root)));
            caps.register(Box::new(FsExistsCap::new(&root)));
            caps.register(Box::new(FsListCap::new(&root)));
        }
        if self.env {
            caps.register(Box::new(EnvGetCap::new(self.env_vars)));
        }
        if self.time {
            caps.register(Box::new(TimeNowCap));
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
        #[cfg(feature = "stdlib")]
        if self.stdlib {
            crate::stdlib::register(&mut caps);
        }
        caps
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Filesystem helpers
// ─────────────────────────────────────────────────────────────────────────────

fn resolve_path(root: &str, path: &Value) -> Result<std::path::PathBuf, String> {
    let s = crate::caps::value_as_text(path);
    let p = std::path::Path::new(&s);
    if p.is_absolute() {
        return Err(format!("absolute paths are not allowed: {s}"));
    }
    let root = std::path::Path::new(root);
    let joined = root.join(p);
    let canonical = joined.canonicalize().unwrap_or(joined);
    let root_canonical = root.canonicalize().unwrap_or(root.to_path_buf());
    if !canonical.starts_with(&root_canonical) {
        return Err(format!("path escapes sandbox root: {s}"));
    }
    Ok(canonical)
}

pub struct FsReadCap {
    root: String,
}

impl FsReadCap {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
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
        let path = resolve_path(&self.root, &args[0])?;
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("fs.read {}: {e}", path.display()))?;
        Ok(Some(Value::Str(data)))
    }
}

pub struct FsWriteCap {
    root: String,
}

impl FsWriteCap {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
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
        let path = resolve_path(&self.root, &args[0])?;
        let data = crate::caps::value_as_text(&args[1]);
        std::fs::write(&path, data).map_err(|e| format!("fs.write {}: {e}", path.display()))?;
        Ok(None)
    }
}

pub struct FsExistsCap {
    root: String,
}

impl FsExistsCap {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
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
        let path = resolve_path(&self.root, &args[0])?;
        Ok(Some(Value::Int(if path.exists() { 1 } else { 0 })))
    }
}

pub struct FsListCap {
    root: String,
}

impl FsListCap {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
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
        let path = resolve_path(&self.root, &args[0])?;
        let entries: Vec<Value> = std::fs::read_dir(&path)
            .map_err(|e| format!("fs.list {}: {e}", path.display()))?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok().map(Value::Str))
            .collect();
        Ok(Some(Value::Array(entries)))
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
            Value::Array(a) => a.iter().map(crate::caps::value_as_text).collect(),
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
        assert!(caps.get("process.exec").is_some());
        assert!(caps.get("crypto.sha256").is_some());
        assert!(caps.get("crypto.random").is_some());
        assert!(caps.get("missing").is_none());
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
                Value::Array(vec![Value::Str("hello".to_string())]),
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
