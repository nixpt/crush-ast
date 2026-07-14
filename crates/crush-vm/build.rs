//! Build script for crush-vm.
//!
//! Compiles the example C plugin (`example_c_plugin.c`) into a shared library
//! so the `test_ffi_gateway_cap` integration test can load it without manual
//! setup.
//!
//! If `gcc` is not on PATH the build emits a warning and skips the compilation;
//! the test will then be skipped at runtime (it checks for the file).

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Path to the C example plugin source
    let plugin_src = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("crush-ffi")
        .join("examples")
        .join("example_c_plugin.c");

    if !plugin_src.exists() {
        println!("cargo:warning=example_c_plugin.c not found at {plugin_src:?} — skipping");
        return;
    }

    let so_path = out_dir.join("example_c_plugin.so");
    let crush_ffi_include = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("crush-ffi")
        .join("include");

    let status = Command::new("gcc")
        .args([
            "-shared",
            "-fPIC",
            "-std=c11",
            "-O2",
            "-o",
            so_path.to_str().unwrap(),
            plugin_src.to_str().unwrap(),
            "-I",
            crush_ffi_include.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:rerun-if-changed=../crush-ffi/examples/example_c_plugin.c");
            println!("cargo:rerun-if-changed=../crush-ffi/include/crush_plugin.h");
            // Emit the .so path as an env var for tests
            println!(
                "cargo:rustc-env=EXAMPLE_C_PLUGIN_SO={}",
                so_path.display()
            );
        }
        Ok(s) => {
            println!(
                "cargo:warning=gcc exited with {} — skipping example_c_plugin build",
                s.code().unwrap_or(-1)
            );
        }
        Err(e) => {
            println!("cargo:warning=gcc not found ({e}) — skipping example_c_plugin build");
        }
    }
}
