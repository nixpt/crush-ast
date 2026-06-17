//! VER-02 — CASM bytecode version gate (v1.7 version-boundary enforcement).
//!
//! Exercises [`casm::Program::deserialize`]'s load-time version check against
//! [`casm::CASM_VERSION`] via the public API. Lives as an integration test so it
//! compiles in its own crate against casm's public surface — sidestepping the
//! pre-existing uncompilable inline `ecasm.rs` tests (tracked as EXO-151).

use casm::{CASM_VERSION, Format, Program};

fn program_with_version(v: &str) -> Vec<u8> {
    let mut p = Program::default();
    p.version = v.to_string();
    p.serialize(Format::Json).unwrap()
}

#[test]
fn accepts_supported_version() {
    // The default version is CASM_VERSION ("1.0") — must round-trip and load.
    let bytes = Program::default().serialize(Format::Json).unwrap();
    let prog = Program::deserialize(&bytes, Format::Json).unwrap();
    assert_eq!(prog.version, CASM_VERSION);
}

#[test]
fn accepts_compatible_minor_bump() {
    // Same major (1) as CASM_VERSION → compatible, loads.
    let bytes = program_with_version("1.7");
    assert!(Program::deserialize(&bytes, Format::Json).is_ok());
}

#[test]
fn rejects_incompatible_major() {
    let bytes = program_with_version("2.0");
    let err = Program::deserialize(&bytes, Format::Json).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("version mismatch"), "unexpected error: {msg}");
    assert!(
        msg.contains("2.0") && msg.contains(CASM_VERSION),
        "msg should name found+expected: {msg}"
    );
}

#[test]
fn rejects_malformed_version() {
    let bytes = program_with_version("not-a-version");
    assert!(Program::deserialize(&bytes, Format::Json).is_err());
}

#[test]
fn gate_applies_to_binary_format_too() {
    // The gate must cover every load path, not just JSON.
    let mut p = Program::default();
    p.version = "9.9".to_string();
    let bytes = p.serialize(Format::Binary).unwrap();
    assert!(Program::deserialize(&bytes, Format::Binary).is_err());
}

#[test]
fn check_version_is_independent_of_serialization() {
    let mut p = Program::default();
    p.version = "3.1".to_string();
    assert!(p.check_version().is_err());
    p.version = CASM_VERSION.to_string();
    assert!(p.check_version().is_ok());
}
