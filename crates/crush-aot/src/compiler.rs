//! Rust compiler driver for AOT compilation.
//!
//! Writes generated Rust source to a temp directory, invokes `rustc` to produce
//! a shared library (`.so`/`.dylib`/`.dll`), and returns the path to the artifact.

use anyhow::{Context, Result};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Environment variable that overrides the default optimization setting of
/// [`AotCompiler::new`]. `0`, `false`, `no` or `off` build unoptimized (`-O0`);
/// anything else, or unset, keeps the optimized default. Meant for test suites,
/// which compare program output and don't need optimized code; an explicit
/// [`AotCompiler::with_optimize`] still wins.
pub const OPTIMIZE_ENV: &str = "CRUSH_AOT_OPTIMIZE";

/// Environment variable that sets the cache directory used by
/// [`AotCompiler::new`] (default `$TMPDIR/crush-aot-cache`). CI points it at a
/// directory it persists between runs; [`AotCompiler::with_cache_dir`] still wins.
pub const CACHE_DIR_ENV: &str = "CRUSH_AOT_CACHE_DIR";

/// `CRUSH_AOT_OPTIMIZE` as a bool: `false` only for an explicit off value.
fn optimize_from_env() -> bool {
    optimize_setting(std::env::var(OPTIMIZE_ENV).ok().as_deref())
}

fn optimize_setting(value: Option<&str>) -> bool {
    !value.is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off"))
}

/// Version banner of a compiler, for the cache key. A compiler that can't be run
/// still gets a stable identity; the build itself then reports the real error.
fn version_banner(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(out) => format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => format!("unavailable: {e}"),
    }
}

/// `rustc -vV` (version, commit hash, host, LLVM), computed once per process.
fn rustc_identity() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| version_banner("rustc", &["-vV"]))
}

/// The resolved path and `--version` banner of C compiler `cc`, computed once per
/// process for each `cc`.
fn cc_identity(cc: &str) -> String {
    static IDS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let ids = IDS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(id) = ids.lock().unwrap().get(cc) {
        return id.clone();
    }
    let path = resolve_on_path(cc).map_or_else(|| cc.to_string(), |p| p.display().to_string());
    let id = format!("{path}\n{}", version_banner(cc, &["--version"]));
    ids.lock().unwrap().entry(cc.to_string()).or_insert(id).clone()
}

/// Where `Command::new(program)` would find `program`: itself when it names a
/// path, else the first match on `PATH`.
fn resolve_on_path(program: &str) -> Option<PathBuf> {
    if program.contains(std::path::MAIN_SEPARATOR) {
        return std::fs::canonicalize(program).ok();
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
}

/// Cache key for one build: everything that changes the artifact — the backend,
/// the optimization setting, the compiler's identity and the generated source.
/// Without the first three, an `-O0` and an `-O3` build (or two toolchains)
/// of the same program would share a cache entry.
fn cache_key(backend: &str, optimize: bool, compiler_identity: &str, source: &str) -> String {
    let mut hasher = Sha256::new();
    for field in [backend, if optimize { "opt" } else { "noopt" }, compiler_identity] {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.update(source.as_bytes());
    hex::encode(hasher.finalize())
}

/// Distinguishes concurrent compiles of the *same* source within one process:
/// the pid alone is not enough when several threads race on one cache entry.
static COMPILE_SEQ: AtomicU64 = AtomicU64::new(0);

fn unique_work_dir(prefix: &str) -> PathBuf {
    let seq = COMPILE_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{seq}", std::process::id()))
}

/// Move a freshly built artifact into the shared cache.
///
/// `rename` is atomic within a filesystem, so a concurrent compile that already
/// filled the entry is replaced by a bit-identical artifact. A custom cache dir
/// may live on another filesystem, where `rename` fails with `EXDEV`; copy then.
fn publish_artifact(built: &Path, cached: &Path) -> Result<()> {
    match std::fs::rename(built, cached) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(built, cached)
                .with_context(|| format!("failed to install {} into the AOT cache", built.display()))?;
            let _ = std::fs::remove_file(built);
            Ok(())
        }
    }
}

/// True when `cached` is already built. A hit refreshes the entry's mtime, so a
/// persisted cache can be pruned to what a run used (CI deletes every entry
/// older than the start of the run before saving it).
fn cache_hit(cached: &Path) -> bool {
    if !cached.exists() {
        return false;
    }
    if let Ok(file) = std::fs::File::options().append(true).open(cached) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
    true
}

/// Configuration for the AOT compiler.
pub struct AotCompiler {
    /// Where to store compiled .so files (cached by content hash).
    cache_dir: PathBuf,
    /// Whether to build optimized (`-C opt-level=3 -C lto=thin` / `-O3 -flto`).
    optimize: bool,
}

impl AotCompiler {
    /// Create a new compiler with default settings.
    ///
    /// Builds are optimized unless `CRUSH_AOT_OPTIMIZE` turns that off
    /// ([`OPTIMIZE_ENV`]). The cache directory is `CRUSH_AOT_CACHE_DIR`
    /// ([`CACHE_DIR_ENV`]) when set, else `$TMPDIR/crush-aot-cache` or
    /// `/tmp/crush-aot-cache`.
    pub fn new() -> Self {
        let cache_dir = match std::env::var_os(CACHE_DIR_ENV) {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => {
                let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(tmp).join("crush-aot-cache")
            }
        };
        Self { cache_dir, optimize: optimize_from_env() }
    }

    /// Enable or disable optimization (default: true, or `CRUSH_AOT_OPTIMIZE`).
    pub fn with_optimize(mut self, opt: bool) -> Self {
        self.optimize = opt;
        self
    }

    /// Set a custom cache directory.
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = dir;
        self
    }

    /// Compile a `casm::Program` to a shared library.
    ///
    /// Returns the path to the compiled `.so` (Linux), `.dylib` (macOS), or `.dll` (Windows).
    pub fn compile_casm(
        &self,
        program: &casm::Program,
        module_name: &str,
    ) -> Result<PathBuf> {
        let rust_source = crate::codegen::gen_rust_source(program)?;

        let hash = cache_key("rust", self.optimize, rustc_identity(), &rust_source);
        let hash_short = &hash[..16];

        // Check cache
        let so_name = format!("{module_name}_{hash_short}");
        let so_path = self.cache_dir.join(&so_name).with_extension(so_ext());
        if cache_hit(&so_path) {
            return Ok(so_path);
        }

        // Ensure cache dir exists
        std::fs::create_dir_all(&self.cache_dir)?;

        // Every compile gets a private work dir, and the artifact is built
        // *inside* it. rustc scatters its thin-LTO intermediates next to the
        // `-o` path under names derived from the crate name, so building
        // straight into the shared cache dir lets concurrent compiles of the
        // same source clobber each other's objects ("cannot open ...rcgu.o").
        let work_dir = unique_work_dir(&format!("crush-aot-{module_name}-{hash_short}"));
        std::fs::create_dir_all(&work_dir)?;
        let lib_path = work_dir.join("lib.rs");
        std::fs::write(&lib_path, &rust_source)?;

        let tmp_path = work_dir.join(&so_name).with_extension(so_ext());

        // Compile with rustc
        let mut cmd = Command::new("rustc");
        cmd.arg("--edition").arg("2024");
        cmd.arg("--crate-type").arg("cdylib");
        cmd.arg("--crate-name").arg(&so_name);
        cmd.arg("-o").arg(&tmp_path);

        if self.optimize {
            cmd.arg("-C").arg("opt-level=3");
            cmd.arg("-C").arg("lto=thin");
        } else {
            cmd.arg("-C").arg("opt-level=0");
        }

        cmd.arg(&lib_path);

        // Set working directory so any relative paths work
        cmd.current_dir(&work_dir);

        let output = cmd.output()
            .with_context(|| format!("Failed to run rustc for {module_name}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            let _ = std::fs::remove_dir_all(&work_dir);

            // Print the generated source for debugging
            eprintln!("--- Generated Rust source ({module_name}) ---");
            eprintln!("{rust_source}");
            eprintln!("--- rustc stderr ---");
            eprintln!("{stderr}");
            eprintln!("--- rustc stdout ---");
            eprintln!("{stdout}");

            anyhow::bail!("rustc compilation failed for {module_name}: {stderr}");
        }

        publish_artifact(&tmp_path, &so_path)?;

        let _ = std::fs::remove_dir_all(&work_dir);

        Ok(so_path)
    }

    /// Compile Crush source text directly.
    ///
    /// Parses via `crush_frontend::compile_crush_source()`, then compiles the resulting CASM.
    pub fn compile_source(&self, source: &str, module_name: &str) -> Result<PathBuf> {
        let program = crush_frontend::compile_crush_source(source)
            .context("Failed to compile Crush source")?;
        self.compile_casm(&program, module_name)
    }

    /// Compile a `casm::Program` to a C shared library using `gcc` or `clang`.
    ///
    /// Generates C source, invokes the specified C compiler, and returns the path
    /// to the resulting `.so`. Uses a separate cache namespace (`c-` prefix).
    pub fn compile_c(
        &self,
        program: &casm::Program,
        module_name: &str,
        cc: &str,
    ) -> Result<PathBuf> {
        let c_source = crate::codegen_c::gen_c_source(program)?;

        let hash = cache_key(&format!("c:{cc}"), self.optimize, &cc_identity(cc), &c_source);
        let hash_short = &hash[..16];

        let so_name = format!("c_{module_name}_{hash_short}");
        let so_path = self.cache_dir.join(&so_name).with_extension(so_ext());
        if cache_hit(&so_path) {
            return Ok(so_path);
        }

        std::fs::create_dir_all(&self.cache_dir)?;

        let work_dir = unique_work_dir(&format!("crush-aot-c-{module_name}-{hash_short}"));
        std::fs::create_dir_all(&work_dir)?;
        let c_path = work_dir.join("lib.c");
        std::fs::write(&c_path, &c_source)?;

        let mut cmd = Command::new(cc);
        cmd.args(["-shared", "-fPIC", "-std=c99"]);
        if self.optimize {
            cmd.args(["-O3", "-flto"]);
        } else {
            cmd.arg("-O0");
        }
        let tmp_path = work_dir.join(&so_name).with_extension(so_ext());
        cmd.arg("-o").arg(&tmp_path);
        cmd.arg(&c_path);

        // Link math library for fmod
        cmd.arg("-lm");

        cmd.current_dir(&work_dir);

        let output = cmd.output()
            .with_context(|| format!("Failed to run {cc} for {module_name}"))?;

        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&work_dir);
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("--- Generated C source ({module_name}) ---");
            eprintln!("{c_source}");
            eprintln!("--- {cc} stderr ---");
            eprintln!("{stderr}");
            anyhow::bail!("{cc} compilation failed for {module_name}: {stderr}");
        }

        publish_artifact(&tmp_path, &so_path)?;

        let _ = std::fs::remove_dir_all(&work_dir);
        Ok(so_path)
    }
}

impl Default for AotCompiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the platform-appropriate shared library extension.
fn so_ext() -> &'static str {
    if cfg!(target_os = "linux") {
        "so"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimize_env_turns_off_only_for_an_explicit_off_value() {
        for off in ["0", "false", "FALSE", "no", "off", " 0 "] {
            assert!(!optimize_setting(Some(off)), "{off:?}");
        }
        for on in [None, Some(""), Some("1"), Some("true"), Some("3")] {
            assert!(optimize_setting(on), "{on:?}");
        }
    }

    #[test]
    fn cache_key_separates_backend_mode_and_toolchain() {
        let base = cache_key("rust", true, "rustc 1.0", "fn main() {}");
        assert_eq!(base, cache_key("rust", true, "rustc 1.0", "fn main() {}"));
        assert_ne!(base, cache_key("rust", false, "rustc 1.0", "fn main() {}"), "opt level");
        assert_ne!(base, cache_key("rust", true, "rustc 1.1", "fn main() {}"), "toolchain");
        assert_ne!(base, cache_key("c:gcc", true, "rustc 1.0", "fn main() {}"), "backend");
        assert_ne!(base, cache_key("rust", true, "rustc 1.0", "fn main() { }"), "source");
        // Length-prefixed fields: moving bytes across a field boundary changes the key.
        assert_ne!(cache_key("ab", true, "c", "x"), cache_key("a", true, "bc", "x"));
    }

    #[test]
    fn compiler_identities_are_stable_within_a_process() {
        assert_eq!(rustc_identity(), rustc_identity());
        assert!(rustc_identity().contains("rustc"), "{}", rustc_identity());
        assert_eq!(cc_identity("no-such-cc-xyz"), cc_identity("no-such-cc-xyz"));
        assert!(cc_identity("no-such-cc-xyz").contains("unavailable"));
    }
}
