//! Rust compiler driver for AOT compilation.
//!
//! Writes generated Rust source to a temp directory, invokes `rustc` to produce
//! a shared library (`.so`/`.dylib`/`.dll`), and returns the path to the artifact.

use anyhow::{Context, Result};
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Configuration for the AOT compiler.
pub struct AotCompiler {
    /// Where to store compiled .so files (cached by content hash).
    cache_dir: PathBuf,
    /// Whether to pass `-C opt-level=3` to rustc.
    optimize: bool,
}

impl AotCompiler {
    /// Create a new compiler with default settings.
    ///
    /// The cache directory defaults to `$TMPDIR/crush-aot-cache` or `/tmp/crush-aot-cache`.
    pub fn new() -> Self {
        let cache_dir = std::env::var("TMPDIR")
            .unwrap_or_else(|_| "/tmp".to_string());
        let cache_dir = PathBuf::from(cache_dir).join("crush-aot-cache");
        Self { cache_dir, optimize: true }
    }

    /// Enable or disable optimization (default: true).
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
        let rust_source = crate::codegen::gen_rust_source(program);

        // Content-hash for cache key
        let mut hasher = Sha256::new();
        hasher.update(rust_source.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let hash_short = &hash[..16];

        // Check cache
        let so_name = format!("{module_name}_{hash_short}");
        let so_path = self.cache_dir.join(&so_name).with_extension(so_ext());
        if so_path.exists() {
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
        let c_source = crate::codegen_c::gen_c_source(program);

        let mut hasher = Sha256::new();
        hasher.update(format!("c:{cc}:").as_bytes());
        hasher.update(c_source.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let hash_short = &hash[..16];

        let so_name = format!("c_{module_name}_{hash_short}");
        let so_path = self.cache_dir.join(&so_name).with_extension(so_ext());
        if so_path.exists() {
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
