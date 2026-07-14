//! Tests for the binary round-trip, disassembler, step quota domain.
//!
//! Auto-extracted from `tests.rs` as part of CRUSHTESTSSPLIT-1 (v2).
//!
//! Each fn preserves its original body verbatim; only the
//! section-banner organizer moved into a sub-file. Multi-line
//! banners are merged into a single classification.

use super::*;
use crate::assembler::{assemble, disassemble};
use crate::vm::{Quotas, Value, run};

// ── binary round-trip ────────────────────────────────────────────────────────

#[test]
fn blob_roundtrip() {
    let prog = assemble("PUSH 42\nHALT", None, None).unwrap();
    let blob = prog.to_blob();
    let prog2 = crate::bytecode::Program::from_blob(&blob).unwrap();
    assert_eq!(prog2.code, prog.code);
    assert_eq!(prog2.consts, prog.consts);
}

// ── disassembler ─────────────────────────────────────────────────────────────

#[test]
fn disassemble_roundtrip() {
    let src = "PUSH 5\nPUSH 3\nADD\nHALT\n";
    let prog = assemble(src, None, None).unwrap();
    let text = disassemble(&prog);
    let prog2 = assemble(&text, None, None).unwrap();
    assert_eq!(prog.code, prog2.code);
}

// ── step quota ───────────────────────────────────────────────────────────────

#[test]
fn step_quota_triggers() {
    let prog = assemble("loop:\nJMP loop", None, None).unwrap();
    let quotas = Quotas {
        max_steps: 10,
        ..Default::default()
    };
    assert!(run(&prog, &quotas).is_err());
}

#[test]
fn test_ffi_gateway_cap() {
    let mut host_caps = crate::host::HostCaps::new();
    host_caps.register(Box::new(crate::plugin::FfiGatewayCap));

    // Load the example C plugin compiled by build.rs.
    let plugin_so = env!("EXAMPLE_C_PLUGIN_SO");

    let asm = format!(
        r#"PUSH_STR "{plugin_so}"
        PUSH_STR "math.add"
        PUSH 10
        PUSH 32
        CAP_CALL "__crush_ffi__" 4
        HALT"#
    );
    eprintln!("plugin_so: {plugin_so}");
    eprintln!("asm:\n{asm}");
    let prog = assemble(
        &asm,
        Some(&["__crush_ffi__"]),
        None,
    )
    .unwrap();

    let result = crate::vm::run_with_caps(&prog, &Quotas::default(), Some(&host_caps)).unwrap();
    assert_eq!(result.stack, vec![Value::Int(42)]);
}

/// Compile and run test_embed.c linked against libcrush_vm.so.
/// Verifies the C API (crush_vm.h) works from a real C program.
#[test]
fn test_c_embed() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_c = manifest_dir.join("src/tests/test_embed.c");
    if !test_c.exists() {
        eprintln!("Skipping test_c_embed: test_embed.c not found");
        return;
    }
    let include_dir = manifest_dir.join("include");

    // Find libcrush_vm.so
    let so_candidates = [
        std::path::PathBuf::from("/build/debug/libcrush_vm.so"),
        manifest_dir.join("../../target/debug/libcrush_vm.so"),
    ];
    let so_path = so_candidates.iter().find(|p| p.exists()).cloned();
    let so_path = match so_path {
        Some(p) => p,
        None => { eprintln!("Skipping: libcrush_vm.so not found"); return; }
    };

    let out_exe = std::env::temp_dir().join("crush_vm_test_embed");
    let rpath = format!("-Wl,-rpath,{}", so_path.parent().unwrap().display());
    let ldir = so_path.parent().unwrap().to_str().unwrap();
    let idir = include_dir.to_str().unwrap();
    let tc = test_c.to_str().unwrap();
    let oe = out_exe.to_str().unwrap();

    let compile = std::process::Command::new("gcc")
        .args(["-o", oe, tc, "-I", idir, "-L", ldir, "-lcrush_vm", "-ldl", &rpath])
        .output()
        .expect("gcc must be on PATH");
    if !compile.status.success() {
        panic!("gcc failed: {}", String::from_utf8_lossy(&compile.stderr));
    }

    let run = std::process::Command::new(&out_exe)
        .env("LD_LIBRARY_PATH", so_path.parent().unwrap())
        .output()
        .expect("test_embed must run");
    let stdout = String::from_utf8_lossy(&run.stdout);
    if !run.status.success() || !stdout.contains("ALL OK") {
        panic!("C embed test failed ({}):\n{stdout}\n{}", run.status.code().unwrap_or(-1), String::from_utf8_lossy(&run.stderr));
    }
    let _ = std::fs::remove_file(&out_exe);
}

