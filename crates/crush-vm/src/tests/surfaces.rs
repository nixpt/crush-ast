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

// Regression: `strip_comment` used to reset its `esc` flag before checking
// it, so an escaped quote (`\"`) inside a string literal was misread as
// the string's closing quote. Anything after that point — including a
// `;` still logically inside the string — was then treated as outside any
// string and truncated as a line comment. This is exactly the shape of
// the CASM an EXEC_LANG polyglot block emits (its JSON args embed quotes
// and the source code verbatim, which routinely contains `;`).
#[test]
fn quoted_string_survives_escaped_quote_before_semicolon() {
    let value = "a\"b; c\\d\ne".to_string(); // a"b; c\d<newline>e
    let src = format!("PUSH_STR {value:?}\nHALT");
    let prog = assemble(&src, None, None).unwrap();
    assert_eq!(prog.consts, vec![value.clone()]);

    // Round-trips through the disassembler too.
    let text = disassemble(&prog);
    let prog2 = assemble(&text, None, None).unwrap();
    assert_eq!(prog.code, prog2.code);
    assert_eq!(prog.consts, prog2.consts);
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

    // Load the example C plugin compiled by build.rs at OUT_DIR.
    // If it wasn't built (e.g., gcc missing), skip the test.
    let plugin_so = env!("EXAMPLE_C_PLUGIN_SO");
    if !std::path::Path::new(plugin_so).exists() {
        eprintln!("Skipping test_ffi_gateway_cap: example_c_plugin.so not built (gcc missing?)");
        return;
    }
    let asm = format!(
        r#"PUSH_STR "{plugin_so}"
        PUSH_STR "math.add"
        PUSH 10
        PUSH 32
        CAP_CALL "__crush_ffi__" 4
        HALT"#
    );
    let prog = assemble(
        &asm,
        Some(&["__crush_ffi__"]),
        None,
    )
    .unwrap();

    let result = crate::vm::run_with_caps(&prog, &Quotas::default(), Some(&host_caps)).unwrap();
    assert_eq!(result.stack, vec![Value::Int(42)]);
}

