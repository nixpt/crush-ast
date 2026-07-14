//! Build script for crush-vm.
//!
//! Compiles the example C plugin into a shared library so the
//! `test_ffi_gateway_cap` test can load it without manual setup.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let plugin_src = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..").join("crush-ffi").join("examples").join("example_c_plugin.c");
    let crush_ffi_include = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..").join("crush-ffi").join("include");

    if !plugin_src.exists() {
        println!("cargo:warning=example_c_plugin.c not found — skipping");
        return;
    }

    let so_path = out_dir.join("example_c_plugin.so");
    let status = Command::new("gcc")
        .args([
            "-shared", "-fPIC", "-std=c11", "-O2",
            "-o", so_path.to_str().unwrap(),
            plugin_src.to_str().unwrap(),
            "-I", crush_ffi_include.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:rerun-if-changed=../crush-ffi/examples/example_c_plugin.c");
            println!("cargo:rerun-if-changed=../crush-ffi/include/crush_plugin.h");
            println!("cargo:rustc-env=EXAMPLE_C_PLUGIN_SO={}", so_path.display());
        }
        Ok(s) => {
            println!("cargo:warning=gcc exited with {} — skipping plugin build", s.code().unwrap_or(-1));
        }
        Err(e) => {
            println!("cargo:warning=gcc not found ({}) — skipping plugin build", e);
        }
    }
}
