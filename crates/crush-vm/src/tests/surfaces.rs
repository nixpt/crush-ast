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

    // We call __crush_ffi__ with: lib_path, cap_name, args...
    // /tmp/example_c_plugin.so was compiled earlier.
    let prog = assemble(
        r#"PUSH_STR "/tmp/example_c_plugin.so"
        PUSH_STR "math.add"
        PUSH 10
        PUSH 32
        CAP_CALL "__crush_ffi__" 4
        HALT"#,
        Some(&["__crush_ffi__"]),
        None,
    )
    .unwrap();

    let result = crate::vm::run_with_caps(&prog, &Quotas::default(), Some(&host_caps)).unwrap();
    assert_eq!(result.stack, vec![Value::Int(42)]);
}

