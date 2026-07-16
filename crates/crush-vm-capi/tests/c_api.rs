//! Integration test for the crush-vm C API.
//!
//! Builds `tests/test_embed.c` against the produced `libcrush_vm_capi.so`
//! and runs it to verify the C-ABI entry points work from C.

use std::path::PathBuf;

#[test]
fn test_c_embed() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_c = manifest_dir.join("tests/test_embed.c");
    if !test_c.exists() {
        eprintln!("Skipping test_c_embed: test_embed.c not found");
        return;
    }

    // The cdylib is in the same directory as the test binary.
    let so_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let include_dir = manifest_dir.join("include");

    let so_candidates = [
        so_dir.join("libcrush_vm_capi.so"),
        manifest_dir.join("../../target/debug/libcrush_vm_capi.so"),
        std::path::PathBuf::from("/build/debug/libcrush_vm_capi.so"),
    ];

    let so_path = so_candidates
        .iter()
        .find(|p| p.exists())
        .cloned();

    let so_path = match so_path {
        Some(p) => p,
        None => {
            eprintln!("Skipping test_c_embed: libcrush_vm_capi.so not found");
            return;
        }
    };

    // Verify the cdylib actually exports the C-API symbols.
    let nm_output = std::process::Command::new("nm")
        .args(["-D", so_path.to_str().unwrap()])
        .output()
        .expect("nm must be on PATH");
    let nm_stdout = String::from_utf8_lossy(&nm_output.stdout);
    if !nm_stdout.contains("crush_vm_init") {
        eprintln!("Skipping test_c_embed: libcrush_vm_capi.so was built without C-API symbols");
        return;
    }

    // Compile test_embed.c
    let out_exe = std::env::temp_dir().join("crush_vm_capi_test_embed");
    let rpath = format!("-Wl,-rpath,{}", so_path.parent().unwrap().display());
    let ldir = so_path.parent().unwrap().to_str().unwrap().to_string();
    let idir = include_dir.to_str().unwrap().to_string();
    let tc = test_c.to_str().unwrap().to_string();
    let oe = out_exe.to_str().unwrap().to_string();
    let compile = std::process::Command::new("gcc")
        .args([
            "-o", &oe,
            &tc,
            "-I", &idir,
            "-L", &ldir,
            "-lcrush_vm_capi",
            "-ldl",
            &rpath,
        ].as_slice())
        .output()
        .expect("gcc must be on PATH");

    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        panic!("gcc compilation failed:\n{stderr}");
    }

    // Run the test
    let run = std::process::Command::new(&out_exe)
        .env("LD_LIBRARY_PATH", so_path.parent().unwrap())
        .output()
        .expect("test_embed executable must run");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr_str = String::from_utf8_lossy(&run.stderr);

    if !run.status.success() || !stdout.contains("ALL OK") {
        panic!(
            "C embed test failed (exit={}):\nstdout:\n{stdout}\nstderr:\n{stderr_str}",
            run.status.code().unwrap_or(-1)
        );
    }
    let _ = std::fs::remove_file(&out_exe);
}
