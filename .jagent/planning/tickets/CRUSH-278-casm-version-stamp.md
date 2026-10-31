# CRUSH-278 — CASM `version` stamped with the CAST version; walker CASM fails the VER-02 load gate

| Field | Value |
|-------|-------|
| **ID** | CRUSH-278 |
| **Priority** | P2 |
| **Status** | Done — fix commit `9718d29` (CRUSH-277 S3) merged in PR #285 (2026-10-21) |
| **Phase** | CASM load path |
| **Assignee** | unassigned |
| **Filed by** | muse — 2026-10-20, found by CRUSH-277 S3's end-to-end tests |

## Problem

`crush-frontend`'s compiler stamps the compiled CASM program with the
*CAST* version (`crates/crush-frontend/src/compiler.rs`:
`casm_program.version = program.cast_version`). Walker CAST carries
`cast_version` "0.2" by convention (JS/SWC walkers, python walker,
`crush-walker-core` docs), while Crush-native CAST uses "1".

VER-02 added a load-time CASM version gate (`casm::Program::check_version`,
`CASM_VERSION = "1.0"`, enforced inside `Program::deserialize`). Any
walker-compiled CASM therefore fails to reload through `deserialize` —
`version mismatch at casm boundary: expected 1.0, found 0.2` — breaking
the compile-to-file-and-run-later flow for every walker language, JS
included. Crush-native programs pass only because their CAST version
accidentally shares the CASM major.

## Fix

The CASM `version` field is the CASM schema version; stamp it with
`casm::CASM_VERSION` at the compile site (the CAST version is still
available on the CAST side and `Program::lang` records provenance).
No test asserted `casm.version == cast_version`.

Evidence: the CRUSH-277 S3 boa end-to-end tests round-trip a
JS-compiled program through CASM JSON → `casm_to_vm` → CVM1 blob →
scheduler VM and pass.

## Survey 2026-10-31 (status)

Commit `9718d29` is an ancestor of origin/main (via PR #285); `crates/crush-frontend/src/compiler.rs` cites CRUSH-278 at the stamp. Previous status: `` Fixed on `agent/muse/CRUSH-277-s3` (one-line stamp + S3 round-trip tests) — awaiting merge ``.
